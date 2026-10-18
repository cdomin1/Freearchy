//! `freearchy install-system` — our own installer, the Omarchy-ISO analogue.
//!
//! Runs on a live FreeBSD system (stock ISO "Live CD" shell, or our future
//! custom media): partition a disk, lay down ZFS + base system + freearchy,
//! reboot into a working desktop.
//!
//! Interactive wizard via ratatui (install_tui.rs); the pipeline functions
//! below are UI-agnostic — the web installer uses the same `execute`.

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::util;

const MNT: &str = "/mnt";

/// zroot is the live-ISO default; FREEARCHY_POOL overrides it for tests and
//  hosts that already import a pool named zroot.
fn pool() -> String {
    std::env::var("FREEARCHY_POOL").unwrap_or_else(|_| "zroot".into())
}
const REPO_TARBALL: &str =
    "https://github.com/cdomin1/Freearchy/archive/refs/heads/main.tar.gz";

/// Fully-specified install request — produced by the TUI wizard, CLI flags,
/// or the web installer. `execute` runs the same pipeline for all of them.
pub struct InstallSpec {
    pub disk: String,
    pub hostname: String,
    pub username: String,
    pub fullname: Option<String>,
    pub email: Option<String>,
    pub keymap: String,
    pub password: Option<String>,
}

pub fn run(
    disk: Option<String>,
    hostname: Option<String>,
    username: Option<String>,
    yes: bool,
) -> Result<()> {
    util::ensure_freebsd()?;
    ensure_root()?;

    // No flags at all → full wizard (the live-image path).
    if !yes && disk.is_none() && hostname.is_none() && username.is_none() {
        return crate::install_tui::run_wizard();
    }

    // Flag mode: defaults for anything unset; still confirm unless --yes.
    let disk = disk.unwrap_or_else(|| {
        let disks = disks().expect("disk detection");
        if disks.is_empty() {
            eprintln!("error: no candidate disks found (kern.disks empty)");
            std::process::exit(1);
        }
        if disks.len() > 1 {
            eprintln!("multiple disks — pass --disk:");
            for (_, l) in &disks {
                eprintln!("  {l}");
            }
            std::process::exit(1);
        }
        disks[0].0.clone()
    });
    let hostname = hostname.unwrap_or_else(|| "freearchy".into());
    let username = username.unwrap_or_else(|| "freearchy".into());

    if !yes {
        print!("Install FreeBSD + freearchy onto /dev/{disk}? ALL DATA ERASED [y/N] ");
        std::io::stdout().flush()?;
        let mut a = String::new();
        std::io::stdin().read_line(&mut a)?;
        if !a.trim().eq_ignore_ascii_case("y") {
            bail!("aborted");
        }
    }

    let spec = InstallSpec {
        disk,
        hostname,
        username,
        fullname: None,
        email: None,
        keymap: "us".into(),
        password: None,
    };
    execute(&spec, &|m| println!("{m}"))
}

/// The install pipeline. `log` receives milestone messages — the TUI prints
/// them, the web installer buffers them for the progress pane.
pub fn execute(spec: &InstallSpec, log: &dyn Fn(&str)) -> Result<()> {
    let dev = format!("/dev/{}", spec.disk);
    partition(&dev, log)?;
    let datasets = zfs_layout(&dev, log)?;
    extract_base(log)?;
    boot_loader(&dev, log)?;
    system_files(spec)?;
    create_user(spec, log)?;
    install_freearchy(log)?;
    datasets.teardown()?;
    log("Install complete. Remove the media and reboot.");
    Ok(())
}

fn ensure_root() -> Result<()> {
    match util::capture("id", &["-u"]).map(|s| s.trim().to_string()) {
        Some(u) if u == "0" => Ok(()),
        _ => bail!("install-system must run as root"),
    }
}

// --- disk selection --------------------------------------------------------

/// Candidate install targets as (device name, display label),
/// minus the disk backing / and removable pseudo-devices.
pub(crate) fn disks() -> Result<Vec<(String, String)>> {
    // Test hook: FREEARCHY_DISKS="md1:Fake Disk;md2:Other" overrides discovery.
    if let Ok(fake) = std::env::var("FREEARCHY_DISKS") {
        return Ok(fake
            .split(';')
            .filter_map(|e| {
                let (n, l) = e.split_once(':').unwrap_or((e, e));
                (!n.is_empty()).then(|| (n.to_string(), l.to_string()))
            })
            .collect());
    }
    let root = root_disk();
    // Booted from memstick/ISO: root is an md(4) vnode backed by the media
    // device — exclude it so the stick itself is never offered as a target.
    let media = root
        .as_deref()
        .filter(|d| d.starts_with("md"))
        .and_then(md_backing_disk);
    let names = util::capture("sysctl", &["-n", "kern.disks"]).unwrap_or_default();
    Ok(names
        .split_whitespace()
        .filter(|d| {
            !d.starts_with("cd")
                && !d.starts_with("md")
                && Some(*d) != root.as_deref()
                && Some(*d) != media.as_deref()
        })
        .map(|d| (d.to_string(), format!("{d} {}", disk_size(d))))
        .collect())
}

fn disk_size(disk: &str) -> String {
    // `diskinfo nda0` → fields: name sectorsize mediasize ...
    util::capture("diskinfo", &[disk])
        .and_then(|l| l.split_whitespace().nth(2).map(String::from))
        .and_then(|b| b.parse::<u64>().ok())
        .map(|b| format!("({} GiB)", b / (1 << 30)))
        .unwrap_or_default()
}

/// Base disk name backing / — so we never offer the media we booted from.
/// /dev/nda0p3 → nda0 ; /dev/da0s1a → da0 ; /dev/mmcsd0p2 → mmcsd0
fn root_disk() -> Option<String> {
    let src = util::capture("sh", &["-c", "mount | awk '$3==\"/\"{print $1}'"])?;
    Some(base_disk_name(src.trim().trim_start_matches("/dev/")))
}

/// Disk backing an md(4) vnode device — e.g. md0 → /dev/da0s2 → "da0".
fn md_backing_disk(md: &str) -> Option<String> {
    // `mdconfig -l -u 0` → "md0  vnode  1.4G  /dev/da0s2  -"
    let out = util::capture("mdconfig", &["-l", "-u", md.trim_start_matches("md")])?;
    let path = out.split_whitespace().nth(3)?;
    path.strip_prefix("/dev/").map(base_disk_name)
}

/// nda0p3 → nda0 ; da0s1a → da0 ; mmcsd0p2 → mmcsd0
fn base_disk_name(name: &str) -> String {
    let bytes = name.as_bytes();
    for i in 0..bytes.len() {
        // partition suffix starts at 'p' or 's' followed by a digit,
        // where a digit already appeared earlier in the name
        let is_sep = (bytes[i] == b'p' || bytes[i] == b's')
            && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)
            && bytes[..i].iter().any(u8::is_ascii_digit);
        if is_sep {
            return name[..i].to_string();
        }
    }
    name.to_string()
}

// --- pipeline steps --------------------------------------------------------

fn partition(dev: &str, log: &dyn Fn(&str)) -> Result<()> {
    log(&format!("Partitioning {dev}"));
    let _ = util::run("gpart", &["destroy", "-F", dev]); // ok if no table
    util::run("gpart", &["create", "-s", "gpt", dev])?;
    util::run("gpart", &["add", "-a", "4k", "-t", "efi", "-s", "260m", "-l", "efiboot", dev])?;
    util::run("gpart", &["add", "-a", "4k", "-t", "freebsd-zfs", "-l", "zroot", dev])?;
    util::run("newfs_msdos", &[
        "-F", "32", "-c", "1", "-L", "EFISYS", &format!("{dev}p1"),
    ])
}

/// Create the pool + datasets under /mnt. Returns a guard-ish struct with the
/// teardown step kept explicit (no Drop — failures shouldn't auto-export).
struct PoolGuard;

impl PoolGuard {
    fn teardown(self) -> Result<()> {
        let _ = util::run("umount", &[&format!("{MNT}/boot/efi")]);
        util::run("zpool", &["export", &pool()])
    }
}

fn pool_exists(p: &str) -> bool {
    util::capture("zpool", &["list", "-H", "-o", "name"])
        .map(|s| s.lines().any(|l| l.trim() == p))
        .unwrap_or(false)
}

/// Is `p`'s vdev the `{dev}p2` we just partitioned? True only for a pool a
/// previous failed run created on this same disk this boot.
fn pool_on_dev(p: &str, dev: &str) -> bool {
    let want = format!("/dev/{dev}p2");
    util::capture("zpool", &["status", "-P", p])
        .map(|s| s.split_whitespace().any(|t| t == want))
        .unwrap_or(false)
}

fn zfs_layout(dev: &str, log: &dyn Fn(&str)) -> Result<PoolGuard> {
    let p = pool();
    log(&format!("Creating zpool {p}"));
    if pool_exists(&p) {
        // Retry on the same disk → wipe and start over; a zroot living on
        // ANOTHER disk is not ours to touch.
        if pool_on_dev(&p, dev) {
            let _ = util::run("zpool", &["destroy", "-f", &p]);
        } else {
            bail!("pool '{p}' is already imported on another disk — export it first or set FREEARCHY_POOL");
        }
    }
    util::run("zpool", &[
        "create", "-f", "-o", &format!("altroot={MNT}"),
        "-O", "compress=lz4", "-O", "atime=off", "-O", "mountpoint=none",
        &p, &format!("{dev}p2"),
    ])?;

    // Standard FreeBSD-on-ZFS dataset layout (bsdinstall's).
    for args in [
        format!("create -o mountpoint=none {p}/ROOT"),
        format!("create -o mountpoint=/ {p}/ROOT/default"),
        format!("create -o mountpoint=/tmp -o exec=on -o setuid=off {p}/tmp"),
        format!("create -o mountpoint=/usr -o canmount=off {p}/usr"),
        format!("create {p}/usr/home"),
        format!("create -o mountpoint=/var -o canmount=off {p}/var"),
        format!("create -o exec=off -o setuid=off {p}/var/audit"),
        format!("create -o exec=off -o setuid=off {p}/var/crash"),
        format!("create -o exec=off -o setuid=off {p}/var/log"),
        format!("create -o atime=on {p}/var/mail"),
        format!("create -o exec=on -o setuid=off {p}/var/tmp"),
    ] {
        let argv: Vec<&str> = args.split_whitespace().collect();
        util::run("zfs", &argv)?;
    }
    util::run("zpool", &["set", &format!("bootfs={p}/ROOT/default"), &p])?;
    Ok(PoolGuard)
}

/// Extract base.txz/kernel.txz — from the media's dist dir if present,
/// else fetch matching the live system's release.
fn extract_base(log: &dyn Fn(&str)) -> Result<()> {
    let distdir = dist_dir()?;
    log(&format!("Extracting base system from {distdir}"));
    for set in ["kernel.txz", "base.txz"] {
        let path = format!("{distdir}/{set}");
        if !Path::new(&path).exists() {
            bail!("missing dist set: {path}");
        }
        util::run("tar", &["-xf", &path, "-C", MNT])?;
    }
    Ok(())
}

fn dist_dir() -> Result<String> {
    const MEDIA_DIST: &str = "/usr/freebsd-dist";
    // 15.x media may carry only a MANIFEST (pkgbase .pkgs live elsewhere) —
    // require the actual sets before treating the dir as usable.
    let has_sets = ["kernel.txz", "base.txz"]
        .iter()
        .all(|s| Path::new(&format!("{MEDIA_DIST}/{s}")).exists());
    if has_sets {
        return Ok(MEDIA_DIST.into());
    }
    // Live CD / custom media: fetch the dists for the running release.
    // Layout: download.freebsd.org/releases/<family>/<arch>/<rel>/<set>
    // e.g. releases/arm64/aarch64/15.1-RELEASE/base.txz
    let rel = util::capture("uname", &["-r"]).context("uname -r")?;
    let rel = rel.trim(); // e.g. 15.1-RELEASE
    let arch = util::capture("uname", &["-m"]).context("uname -m")?;
    let arch = arch.trim();
    let family = if arch == "aarch64" { "arm64" } else { arch };
    let dir = "/tmp/freebsd-dist";
    fs::create_dir_all(dir)?;
    for set in ["kernel.txz", "base.txz"] {
        let url = format!(
            "https://download.freebsd.org/releases/{family}/{arch}/{rel}/{set}"
        );
        util::run("fetch", &["-o", &format!("{dir}/{set}"), &url])?;
    }
    Ok(dir.into())
}

fn boot_loader(dev: &str, log: &dyn Fn(&str)) -> Result<()> {
    log("Installing EFI loader");
    let efi = format!("{MNT}/boot/efi");
    fs::create_dir_all(&efi)?;
    util::run("mount", &["-t", "msdosfs", &format!("{dev}p1"), &efi])?;

    // Removable-media path — always bootable without NVRAM entries.
    let boot_name = match util::capture("uname", &["-m"]).as_deref() {
        Some("aarch64") => "BOOTAA64.EFI",
        _ => "BOOTX64.EFI",
    };
    for dst_dir in ["EFI/BOOT", "EFI/freebsd"] {
        let dst = Path::new(&efi).join(dst_dir);
        fs::create_dir_all(&dst)?;
        let target = if dst_dir == "EFI/BOOT" {
            dst.join(boot_name)
        } else {
            dst.join("loader.efi")
        };
        fs::copy(format!("{MNT}/boot/loader.efi"), &target)
            .with_context(|| format!("copying loader to {}", target.display()))?;
    }

    // zpool.cache so the loader finds the pool pre-mount.
    fs::create_dir_all(format!("{MNT}/boot/zfs"))?;
    let _ = fs::copy("/boot/zfs/zpool.cache", format!("{MNT}/boot/zfs/zpool.cache"));

    fs::write(
        format!("{MNT}/boot/loader.conf"),
        format!(
            "zfs_load=\"YES\"\nvfs.root.mountfrom=\"zfs:{}/ROOT/default\"\n\
             kern.geom.label.disk_ident.enable=\"0\"\nkern.geom.label.gptid.enable=\"0\"\n\
             # power: put driverless PCI devices into D3\n\
             hw.pci.do_power_nodriver=\"3\"\n",
            pool()
        ),
    )?;

    // fstab: ESP for future loader updates.
    fs::write(
        format!("{MNT}/etc/fstab"),
        format!("/dev/gpt/efiboot\t/boot/efi\tmsdosfs\trw\t\t2\t2\n"),
    )?;
    Ok(())
}

/// KMS module for the detected GPU — linuxkpi-backed drm-kmod drivers.
/// pciconf: vga lines look like `vgapci0@pci0:0:2:0: class=0x030000 ... vendor=0x8086`
fn system_files(spec: &InstallSpec) -> Result<()> {
    let kld_list = crate::gpu::kld_list();
    let kld = if kld_list.is_empty() {
        String::new()
    } else {
        format!("kld_list=\"{kld_list}\"\n")
    };
    fs::write(
        format!("{MNT}/etc/rc.conf"),
        format!(
            "hostname=\"{}\"\nzfs_enable=\"YES\"\nsshd_enable=\"YES\"\n\
             keymap=\"{}\"\nifconfig_DEFAULT=\"SYNCDHCP\"\n\
             freearchy_firstboot_enable=\"YES\"\npowerdxx_enable=\"YES\"\n{}",
            spec.hostname, spec.keymap, kld
        ),
    )?;
    crate::gpu::write_xorg_conf(MNT);
    sleep_setup();
    // git identity — the wizard's email/fullname feed ~/.gitconfig
    if let Some(email) = &spec.email {
        let name = spec.fullname.as_deref().unwrap_or(&spec.username);
        fs::write(
            format!("{MNT}/home/{}/.gitconfig", spec.username),
            format!("[user]\n\tname = {name}\n\temail = {email}\n"),
        )?;
    }
    Ok(())
}

/// Best suspend state this machine supports: real S3 (suspend-to-RAM)
/// beats s2idle; s2idle is the only option on S0ix-only laptops.
/// 15.x advertises via kern.power.supported_stype; older kernels via
/// hw.acpi.supported_sleep_states.
fn best_sleep_state() -> Option<String> {
    for mib in ["kern.power.supported_stype", "hw.acpi.supported_sleep_states"] {
        if let Some(states) = util::capture("sysctl", &["-n", mib]) {
            let s: Vec<&str> = states.split_whitespace().collect();
            if s.contains(&"S3") {
                return Some("S3".into());
            }
            if s.iter().any(|t| t.eq_ignore_ascii_case("s2idle")) {
                return Some("s2idle".into());
            }
        }
    }
    None
}

/// Persist suspend config in the target: lid/power/sleep-button go to
/// the best state; rc.suspend/rc.resume carry the device workarounds.
fn sleep_setup() {
    let state = best_sleep_state().unwrap_or_else(|| "NONE".into());
    let _ = fs::write(
        format!("{MNT}/etc/sysctl.conf"),
        format!(
            "# Freearchy suspend config\n\
             hw.acpi.lid_switch_state={state}\n\
             hw.acpi.sleep_button_state={state}\n\
             hw.acpi.power_button_state=S5\n"
        ),
    );
    for (f, body) in [
        ("rc.suspend", include_str!("../../config/power/rc.suspend")),
        ("rc.resume", include_str!("../../config/power/rc.resume")),
    ] {
        let dst = format!("{MNT}/etc/{f}");
        let _ = fs::write(&dst, body);
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dst, fs::Permissions::from_mode(0o755));
    }
    // power profile script + devd AC hook into the target
    let _ = fs::create_dir_all(format!("{MNT}/usr/local/libexec"));
    let _ = fs::create_dir_all(format!("{MNT}/usr/local/etc/devd"));
    let _ = fs::write(
        format!("{MNT}/usr/local/libexec/freearchy-power"),
        include_str!("../../config/power/freearchy-power"),
    );
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(
        format!("{MNT}/usr/local/libexec/freearchy-power"),
        fs::Permissions::from_mode(0o755),
    );
    let _ = fs::write(
        format!("{MNT}/usr/local/etc/devd/freearchy-power.conf"),
        include_str!("../../config/power/devd.conf"),
    );
}

fn create_user(spec: &InstallSpec, log: &dyn Fn(&str)) -> Result<()> {
    let username = &spec.username;
    let gecos = spec.fullname.as_deref().unwrap_or("freearchy user");
    log(&format!("Creating user {username}"));
    util::run("chroot", &[
        MNT, "pw", "useradd", "-n", username, "-m", "-s", "/bin/sh",
        "-G", "wheel,video", "-c", gecos,
    ])?;
    if let Some(pass) = spec.password.as_deref() {
        // via stdin — never put it on a command line (visible in ps).
        run_stdin("chroot", &[MNT, "pw", "mod", "user", username, "-h", "0"], pass)?;
    } else {
        log(&format!("note: account is locked — run `passwd {username}` after login"));
    }
    Ok(())
}

/// Run a command with a string piped to stdin (for secrets).
fn run_stdin(cmd: &str, args: &[&str], input: &str) -> Result<()> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {cmd}"))?;
    child
        .stdin
        .as_mut()
        .context("stdin")?
        .write_all(input.as_bytes())?;
    let status = child.wait()?;
    if !status.success() {
        bail!("{cmd} exited with {status}");
    }
    Ok(())
}

/// Deliver the freearchy repo + firstboot hook into the installed system.
fn install_freearchy(log: &dyn Fn(&str)) -> Result<()> {
    log("Installing freearchy payload");
    let dst = format!("{MNT}/usr/local/freearchy");
    let local = util::repo_root().unwrap_or_default();

    if local.join("boot.sh").exists() {
        // Installer built from a checkout — copy it in.
        util::run("cp", &["-R", local.to_str().unwrap(), &dst])?;
    } else {
        util::run("fetch", &["-o", "/tmp/freearchy.tar.gz", REPO_TARBALL])?;
        util::run("tar", &["-xzf", "/tmp/freearchy.tar.gz", "-C", "/tmp"])?;
        // GitHub tarballs extract to <name>-main/
        for cand in ["/tmp/freearchy-main", "/tmp/freearchy-freebsd"] {
            if Path::new(cand).is_dir() {
                util::run("mv", &[cand, &dst])?;
                break;
            }
        }
    }

    // Firstboot service (from the repo we just placed).
    let rc_src = format!("{dst}/image/freearchy-firstboot");
    let rc_dst = format!("{MNT}/usr/local/etc/rc.d");
    fs::create_dir_all(&rc_dst)?;
    if Path::new(&rc_src).exists() {
        fs::copy(&rc_src, format!("{rc_dst}/freearchy-firstboot"))?;
        util::run("chmod", &["+x", &format!("{rc_dst}/freearchy-firstboot")])?;
    } else {
        log("warn: no freearchy-firstboot in payload — skipping autostart");
    }
    Ok(())
}

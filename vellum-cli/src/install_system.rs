//! `vellum install-system` — our own installer, the Omarchy-ISO analogue.
//!
//! Runs on a live FreeBSD system (stock ISO "Live CD" shell, or our future
//! custom media): partition a disk, lay down ZFS + base system + vellum,
//! reboot into a working desktop.
//!
//! Interactive wizard via ratatui (install_tui.rs); the pipeline functions
//! below are UI-agnostic — the web installer uses the same `execute`.

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::util;

const MNT: &str = "/mnt";

/// zroot is the live-ISO default; VELLUM_POOL overrides it for tests and
//  hosts that already import a pool named zroot.
fn pool() -> String {
    std::env::var("VELLUM_POOL").unwrap_or_else(|_| "zroot".into())
}
const REPO_TARBALL: &str =
    "https://github.com/cdomin1/vellum/archive/refs/heads/main.tar.gz";

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
    let disk = match disk {
        Some(d) => d,
        None => {
            let disks = disks()?;
            match disks.as_slice() {
                [] => bail!("no candidate disks found (kern.disks empty)"),
                [(name, _)] => name.clone(),
                _ => {
                    let list: Vec<&str> = disks.iter().map(|(_, l)| l.as_str()).collect();
                    bail!("multiple disks — pass --disk:\n  {}", list.join("\n  "));
                }
            }
        }
    };
    let hostname = hostname.unwrap_or_else(|| "vellum".into());
    let username = username.unwrap_or_else(|| "vellum".into());

    if !yes {
        print!("Install FreeBSD + vellum onto /dev/{disk}? ALL DATA ERASED [y/N] ");
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
    validate(spec)?;
    let dev = format!("/dev/{}", spec.disk);
    partition(&dev, log)?;
    let datasets = zfs_layout(&dev, log)?;
    extract_base(log)?;
    boot_loader(&dev, log)?;
    system_files(spec)?;
    create_user(spec, log)?;
    install_vellum(log)?;
    datasets.teardown()?;
    log("Install complete. Remove the media and reboot.");
    Ok(())
}

pub(crate) fn valid_username(u: &str) -> bool {
    let mut chars = u.chars();
    u.len() <= 32
        && chars.next().is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

pub(crate) fn valid_hostname(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 63
        && !h.starts_with(['-', '.'])
        && !h.ends_with(['-', '.'])
        && h.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
}

/// Every field lands in argv, rc.conf or a path — reject anything that
/// could break out of those (all three front-ends funnel through here).
fn validate(spec: &InstallSpec) -> Result<()> {
    if spec.disk.is_empty() || !spec.disk.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("invalid disk name '{}'", spec.disk);
    }
    if !valid_username(&spec.username) {
        bail!("invalid username '{}' (lower-case letters, digits, _ and -)", spec.username);
    }
    if !valid_hostname(&spec.hostname) {
        bail!("invalid hostname '{}' (lower-case letters, digits, - and .)", spec.hostname);
    }
    if !spec.keymap.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        bail!("invalid keymap '{}'", spec.keymap);
    }
    let bad = |s: &Option<String>| s.as_deref().is_some_and(|v| v.contains(['\n', ':', '"']));
    if bad(&spec.fullname) || bad(&spec.email) {
        bail!("full name / email may not contain newlines, ':' or '\"'");
    }
    Ok(())
}

fn ensure_root() -> Result<()> {
    match util::capture("id", &["-u"]).map(|s| s.trim().to_string()) {
        Some(u) if u == "0" => Ok(()),
        _ => bail!("install-system must run as root"),
    }
}

/// Run a pipeline command with its output captured, so child stdout/stderr
/// never paints over the TUI. Failures carry the tail of stderr.
fn sh(cmd: &str, args: &[&str]) -> Result<()> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to spawn {cmd}"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = stderr.lines().rev().take(4).collect();
    let tail: Vec<&str> = tail.into_iter().rev().collect();
    bail!("{cmd} {} failed ({}): {}", args.join(" "), out.status, tail.join(" | "))
}

// --- disk selection --------------------------------------------------------

/// Candidate install targets as (device name, display label),
/// minus the disk backing / and removable pseudo-devices.
pub(crate) fn disks() -> Result<Vec<(String, String)>> {
    // Test hook: VELLUM_DISKS="md1:Fake Disk;md2:Other" overrides discovery.
    if let Ok(fake) = std::env::var("VELLUM_DISKS") {
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
    let _ = sh("gpart", &["destroy", "-F", dev]); // ok if no table
    sh("gpart", &["create", "-s", "gpt", dev])?;
    sh("gpart", &["add", "-a", "4k", "-t", "efi", "-s", "260m", "-l", "efiboot", dev])?;
    sh("gpart", &["add", "-a", "4k", "-t", "freebsd-zfs", "-l", "zroot", dev])?;
    sh("newfs_msdos", &["-F", "32", "-c", "1", "-L", "EFISYS", &format!("{dev}p1")])
}

/// Create the pool + datasets under /mnt. Returns a guard-ish struct with the
/// teardown step kept explicit (no Drop — failures shouldn't auto-export).
struct PoolGuard;

impl PoolGuard {
    fn teardown(self) -> Result<()> {
        let _ = sh("umount", &[&format!("{MNT}/boot/efi")]);
        sh("zpool", &["export", &pool()])
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
    let want = format!("{dev}p2");
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
            let _ = sh("zpool", &["destroy", "-f", &p]);
        } else {
            bail!("pool '{p}' is already imported on another disk — export it first or set VELLUM_POOL");
        }
    }
    sh("zpool", &[
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
        sh("zfs", &argv)?;
    }
    sh("zpool", &["set", &format!("bootfs={p}/ROOT/default"), &p])?;
    Ok(PoolGuard)
}

/// Extract base.txz/kernel.txz — from the media's dist dir if present,
/// else fetch matching the live system's release.
fn extract_base(log: &dyn Fn(&str)) -> Result<()> {
    let distdir = dist_dir(log)?;
    log(&format!("Extracting base system from {distdir}"));
    for set in ["kernel.txz", "base.txz"] {
        let path = format!("{distdir}/{set}");
        if !Path::new(&path).exists() {
            bail!("missing dist set: {path}");
        }
        sh("tar", &["-xf", &path, "-C", MNT])?;
    }
    if distdir.starts_with(MNT) {
        let _ = fs::remove_dir_all(&distdir); // staged download — ~1GB
    }
    Ok(())
}

fn dist_dir(log: &dyn Fn(&str)) -> Result<String> {
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
    // patch levels (15.1-RELEASE-p2) share the release's dist sets
    let rel = rel.trim().split("-p").next().unwrap_or_default().to_string();
    let arch = util::capture("uname", &["-m"]).context("uname -m")?;
    let arch = arch.trim();
    let family = if arch == "aarch64" { "arm64" } else { arch };
    // Stage on the target disk — /tmp on install media is RAM-backed and
    // too small for kernel.txz+base.txz (~1GB).
    let dir = format!("{MNT}/freebsd-dist");
    fs::create_dir_all(&dir)?;
    for set in ["kernel.txz", "base.txz"] {
        log(&format!("Downloading {set} ({rel}, {arch})"));
        let url = format!("https://download.freebsd.org/releases/{family}/{arch}/{rel}/{set}");
        sh("fetch", &["-o", &format!("{dir}/{set}"), &url])?;
    }
    Ok(dir)
}

fn boot_loader(dev: &str, log: &dyn Fn(&str)) -> Result<()> {
    log("Installing EFI loader");
    let efi = format!("{MNT}/boot/efi");
    fs::create_dir_all(&efi)?;
    sh("mount", &["-t", "msdosfs", &format!("{dev}p1"), &efi])?;

    // Removable-media path — always bootable without NVRAM entries.
    let boot_name = match util::capture("uname", &["-m"]).as_deref().map(str::trim) {
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

    // zpool.cache so the loader finds the pool pre-mount. altroot implies
    // cachefile=none, so point the pool at the target's cache explicitly
    // (what bsdinstall does).
    fs::create_dir_all(format!("{MNT}/boot/zfs"))?;
    sh("zpool", &["set", &format!("cachefile={MNT}/boot/zfs/zpool.cache"), &pool()])?;

    fs::write(
        format!("{MNT}/boot/loader.conf"),
        format!(
            "zfs_load=\"YES\"\nvfs.root.mountfrom=\"zfs:{}/ROOT/default\"\n\
             kern.geom.label.disk_ident.enable=\"0\"\nkern.geom.label.gptid.enable=\"0\"\n\
             # power: put driverless PCI devices into D3\n\
             hw.pci.do_power_nodriver=\"3\"\n{}",
            pool(),
            crate::gpu::loader_conf_lines()
                .iter()
                .map(|l| format!("{l}\n"))
                .collect::<String>()
        ),
    )?;

    // fstab: ESP for future loader updates.
    fs::write(
        format!("{MNT}/etc/fstab"),
        "/dev/gpt/efiboot\t/boot/efi\tmsdosfs\trw\t\t2\t2\n",
    )?;
    Ok(())
}

/// Extra kmods for Apple hardware: `wsp`/`atp` drive the multitouch
/// trackpads (base modules, no firmware needed).
fn apple_klds() -> Vec<&'static str> {
    let vendor = util::capture("sysctl", &["-n", "hw.vendor"]).unwrap_or_default();
    let model = util::capture("sysctl", &["-n", "hw.model"]).unwrap_or_default();
    if vendor.contains("Apple") || model.starts_with("Mac") {
        vec!["wsp", "atp"]
    } else {
        Vec::new()
    }
}

/// vt(4) keymap → X11 (layout, variant), so the desktop types like the
/// console the user picked in the wizard.
fn xkb_for(keymap: &str) -> (&str, &str) {
    match keymap {
        "us.dvorak" => ("us", "dvorak"),
        "ca-fr" => ("ca", "fr"),
        "ch" => ("ch", "de"),
        other => (other.split(['.', '-']).next().unwrap_or("us"), ""),
    }
}

fn system_files(spec: &InstallSpec) -> Result<()> {
    let mut klds: Vec<String> = crate::gpu::kld_list()
        .split_whitespace()
        .map(String::from)
        .collect();
    klds.extend(apple_klds().iter().map(|s| s.to_string()));
    let kld = if klds.is_empty() {
        String::new()
    } else {
        format!("kld_list=\"{}\"\n", klds.join(" "))
    };
    fs::write(
        format!("{MNT}/etc/rc.conf"),
        format!(
            "hostname=\"{}\"\nzfs_enable=\"YES\"\nsshd_enable=\"YES\"\n\
             keymap=\"{}\"\nifconfig_DEFAULT=\"SYNCDHCP\"\n\
             vellum_firstboot_enable=\"YES\"\nvellum_firstboot_user=\"{}\"\n\
             powerdxx_enable=\"YES\"\n{}",
            spec.hostname, spec.keymap, spec.username, kld
        ),
    )?;

    // bsdinstall's /home → usr/home link, so `pw useradd -m` lands the
    // home directory on the usr/home dataset.
    let home = Path::new(MNT).join("home");
    if !home.exists() && !home.is_symlink() {
        std::os::unix::fs::symlink("usr/home", &home)?;
    }

    // Admin rights for wheel (doas is installed on first boot; the config
    // can land first) + passwordless suspend for the Super+Shift+Esc bind.
    fs::create_dir_all(format!("{MNT}/usr/local/etc"))?;
    fs::write(
        format!("{MNT}/usr/local/etc/doas.conf"),
        "permit persist :wheel\npermit nopass :wheel cmd zzz\npermit nopass :wheel cmd acpiconf\n",
    )?;

    crate::gpu::write_xorg_conf(MNT);
    let (layout, variant) = xkb_for(&spec.keymap);
    crate::gpu::write_xorg_keyboard(MNT, layout, variant);
    sleep_setup();
    Ok(())
}

/// Best suspend state this machine supports: real S3 (suspend-to-RAM)
/// beats s2idle; s2idle is the only option on S0ix-only laptops.
/// 15.x advertises via kern.power.supported_stype; older kernels via
/// hw.acpi.supported_sleep_states.
pub(crate) fn best_sleep_state() -> Option<String> {
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

fn write_exec(path: &str, body: &str) {
    let _ = fs::write(path, body);
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
}

/// Persist suspend config in the target: lid/power/sleep-button go to
/// the best state; rc.suspend/rc.resume carry the device workarounds.
fn sleep_setup() {
    let state = best_sleep_state().unwrap_or_else(|| "NONE".into());
    let _ = fs::write(
        format!("{MNT}/etc/sysctl.conf"),
        format!(
            "# Vellum suspend config\n\
             hw.acpi.suspend_state={state}\n\
             hw.acpi.lid_switch_state={state}\n\
             hw.acpi.sleep_button_state={state}\n\
             hw.acpi.power_button_state=S5\n"
        ),
    );
    write_exec(&format!("{MNT}/etc/rc.suspend"), include_str!("../../config/power/rc.suspend"));
    write_exec(&format!("{MNT}/etc/rc.resume"), include_str!("../../config/power/rc.resume"));
    // power profile script + devd AC hook into the target
    let _ = fs::create_dir_all(format!("{MNT}/usr/local/libexec"));
    let _ = fs::create_dir_all(format!("{MNT}/usr/local/etc/devd"));
    write_exec(
        &format!("{MNT}/usr/local/libexec/vellum-power"),
        include_str!("../../config/power/vellum-power"),
    );
    let _ = fs::write(
        format!("{MNT}/usr/local/etc/devd/vellum-power.conf"),
        include_str!("../../config/power/devd.conf"),
    );
}

fn create_user(spec: &InstallSpec, log: &dyn Fn(&str)) -> Result<()> {
    let username = &spec.username;
    let gecos = spec.fullname.as_deref().filter(|n| !n.is_empty()).unwrap_or("vellum user");
    log(&format!("Creating user {username}"));
    sh("chroot", &[
        MNT, "pw", "useradd", "-n", username, "-m", "-s", "/bin/sh",
        "-G", "wheel,video", "-c", gecos,
    ])?;
    if let Some(pass) = spec.password.as_deref() {
        // via stdin — never put it on a command line (visible in ps).
        // The wizard promises the same password for root.
        for user in [username.as_str(), "root"] {
            run_stdin("chroot", &[MNT, "pw", "usermod", user, "-h", "0"], pass)?;
        }
    } else {
        log(&format!("note: account is locked — run `passwd {username}` after login"));
    }

    // git identity — the wizard's email/fullname feed ~/.gitconfig
    if let Some(email) = spec.email.as_deref().filter(|e| !e.is_empty()) {
        let name = spec.fullname.as_deref().filter(|n| !n.is_empty()).unwrap_or(username);
        fs::write(
            format!("{MNT}/home/{username}/.gitconfig"),
            format!("[user]\n\tname = {name}\n\temail = {email}\n"),
        )?;
        sh("chroot", &[MNT, "chown", &format!("{username}:{username}"),
                       &format!("/home/{username}/.gitconfig")])?;
    }
    Ok(())
}

/// Run a command with a string piped to stdin (for secrets).
fn run_stdin(cmd: &str, args: &[&str], input: &str) -> Result<()> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawn {cmd}"))?;
    child
        .stdin
        .take()
        .context("stdin")?
        .write_all(input.as_bytes())?;
    let status = child.wait()?;
    if !status.success() {
        bail!("{cmd} {} exited with {status}", args.join(" "));
    }
    Ok(())
}

/// Deliver the vellum repo + CLI + firstboot hook into the installed system.
fn install_vellum(log: &dyn Fn(&str)) -> Result<()> {
    log("Installing vellum payload");
    let dst = format!("{MNT}/usr/local/vellum");
    let local = util::repo_root().unwrap_or_default();

    if local.join("boot.sh").exists() {
        // Installer built from a checkout — copy it in.
        sh("cp", &["-R", &local.to_string_lossy(), &dst])?;
    } else {
        sh("fetch", &["-q", "-o", "/tmp/vellum.tar.gz", REPO_TARBALL])?;
        fs::create_dir_all(&dst)?;
        // GitHub tarballs wrap everything in <repo>-<branch>/
        sh("tar", &["-xzf", "/tmp/vellum.tar.gz", "-C", &dst, "--strip-components", "1"])?;
    }

    // The running installer binary is the CLI the new system needs — copy
    // it so first boot doesn't have to compile it.
    fs::create_dir_all(format!("{MNT}/usr/local/bin"))?;
    if let Ok(exe) = std::env::current_exe() {
        let target = format!("{MNT}/usr/local/bin/vellum");
        if fs::copy(&exe, &target).is_ok() {
            let _ = fs::set_permissions(&target, fs::Permissions::from_mode(0o755));
        }
    }

    // Firstboot service (from the repo we just placed).
    let rc_src = format!("{dst}/image/vellum-firstboot");
    let rc_dst = format!("{MNT}/usr/local/etc/rc.d");
    fs::create_dir_all(&rc_dst)?;
    if Path::new(&rc_src).exists() {
        write_exec(&format!("{rc_dst}/vellum-firstboot"), &fs::read_to_string(&rc_src)?);
        // rc(8) only runs `firstboot`-keyword scripts while this sentinel
        // exists, and removes it after that boot.
        fs::write(format!("{MNT}/firstboot"), "")?;
    } else {
        log("warn: no vellum-firstboot in payload — skipping autostart");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_disk_names() {
        assert_eq!(base_disk_name("nda0p3"), "nda0");
        assert_eq!(base_disk_name("da0s1a"), "da0");
        assert_eq!(base_disk_name("mmcsd0p2"), "mmcsd0");
        assert_eq!(base_disk_name("ada1"), "ada1");
    }

    #[test]
    fn usernames() {
        assert!(valid_username("carlos"));
        assert!(valid_username("_svc-1"));
        assert!(!valid_username(""));
        assert!(!valid_username("Carlos"));
        assert!(!valid_username("1abc"));
        assert!(!valid_username("a b"));
    }

    #[test]
    fn hostnames() {
        assert!(valid_hostname("vellum"));
        assert!(valid_hostname("t480.lan"));
        assert!(!valid_hostname("-x"));
        assert!(!valid_hostname("my\"host"));
        assert!(!valid_hostname("Host"));
    }

    #[test]
    fn xkb_mapping() {
        assert_eq!(xkb_for("us"), ("us", ""));
        assert_eq!(xkb_for("us.dvorak"), ("us", "dvorak"));
        assert_eq!(xkb_for("ca-fr"), ("ca", "fr"));
        assert_eq!(xkb_for("de"), ("de", ""));
    }
}

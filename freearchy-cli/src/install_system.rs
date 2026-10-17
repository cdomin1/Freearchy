//! `freearchy install-system` — our own installer, the Omarchy-ISO analogue.
//!
//! Runs on a live FreeBSD system (stock ISO "Live CD" shell, or our future
//! custom media): partition a disk, lay down ZFS + base system + freearchy,
//! reboot into a working desktop.
//!
//! Interactive wizard via dialoguer today; the pipeline functions below are
//! UI-agnostic so a ratatui frontend can replace the prompts later.

use anyhow::{bail, Context, Result};
use dialoguer::{Confirm, Input, Password, Select};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::util;

const MNT: &str = "/mnt";
const POOL: &str = "zroot";
const REPO_TARBALL: &str =
    "https://github.com/USER/freearchy/archive/refs/heads/main.tar.gz";

/// Fully-specified install request — produced by the TUI wizard, CLI flags,
/// or the web installer. `execute` runs the same pipeline for all of them.
pub struct InstallSpec {
    pub disk: String,
    pub hostname: String,
    pub username: String,
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

    let disk = disk.unwrap_or_else(|| pick_disk().expect("disk selection"));

    let hostname = match hostname {
        Some(h) => h,
        None if yes => "freearchy".into(),
        None => Input::new()
            .with_prompt("Hostname")
            .default("freearchy".into())
            .interact_text()?,
    };

    let username = match username {
        Some(u) => u,
        None if yes => "freearchy".into(),
        None => Input::new().with_prompt("Username").interact_text()?,
    };

    let password = if yes {
        None // account created locked; set later with `passwd`
    } else {
        Some(
            Password::new()
                .with_prompt("User password")
                .with_confirmation("Confirm password", "Passwords don't match")
                .interact()?,
        )
    };

    if !yes {
        let ok = Confirm::new()
            .with_prompt(format!(
                "Install FreeBSD + freearchy onto /dev/{disk}? ALL DATA ON IT WILL BE ERASED"
            ))
            .default(false)
            .interact()?;
        if !ok {
            bail!("aborted");
        }
    }

    let spec = InstallSpec { disk, hostname, username, password };
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
    system_files(&spec.hostname)?;
    create_user(&spec.username, spec.password.as_deref(), log)?;
    install_freearchy(log)?;
    datasets.teardown()?;
    log("Install complete. Remove the media and reboot.");
    Ok(())
}

fn ensure_root() -> Result<()> {
    match util::capture("id", &["-u"]).as_deref() {
        Some("0") => Ok(()),
        _ => bail!("install-system must run as root"),
    }
}

// --- disk selection --------------------------------------------------------

fn pick_disk() -> Result<String> {
    let disks = disks()?;
    if disks.is_empty() {
        bail!("no candidate disks found (kern.disks empty)");
    }
    if disks.len() == 1 {
        let (name, label) = disks.into_iter().next().unwrap();
        println!("==> only disk present: {label}");
        return Ok(name);
    }
    let labels: Vec<String> = disks.iter().map(|(_, l)| l.clone()).collect();
    let idx = Select::new()
        .with_prompt("Target disk")
        .items(&labels)
        .interact()?;
    Ok(disks[idx].0.clone())
}

/// Candidate install targets as (device name, display label),
/// minus the disk backing / and removable pseudo-devices.
pub(crate) fn disks() -> Result<Vec<(String, String)>> {
    let root = root_disk();
    let names = util::capture("sysctl", &["-n", "kern.disks"]).unwrap_or_default();
    Ok(names
        .split_whitespace()
        .filter(|d| {
            !d.starts_with("cd") && !d.starts_with("md") && Some(*d) != root.as_deref()
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
    let name = src.trim().trim_start_matches("/dev/");
    let bytes = name.as_bytes();
    for i in 0..bytes.len() {
        // partition suffix starts at 'p' or 's' followed by a digit,
        // where a digit already appeared earlier in the name
        let is_sep = (bytes[i] == b'p' || bytes[i] == b's')
            && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)
            && bytes[..i].iter().any(u8::is_ascii_digit);
        if is_sep {
            return Some(name[..i].to_string());
        }
    }
    Some(name.to_string())
}

// --- pipeline steps --------------------------------------------------------

fn partition(dev: &str, log: &dyn Fn(&str)) -> Result<()> {
    log(&format!("Partitioning {dev}"));
    let _ = util::run("gpart", &["destroy", "-F", dev]); // ok if no table
    util::run("gpart", &["create", "-s", "gpt", dev])?;
    util::run("gpart", &["add", "-a", "4k", "-t", "efi", "-s", "260m", "-l", "efiboot", dev])?;
    util::run("gpart", &["add", "-a", "4k", "-t", "freebsd-zfs", "-l", "zroot", dev])?;
    util::run("newfs_msdos", &["-F", "32", "-L", "EFISYS", &format!("{dev}p1")])
}

/// Create the pool + datasets under /mnt. Returns a guard-ish struct with the
/// teardown step kept explicit (no Drop — failures shouldn't auto-export).
struct PoolGuard;

impl PoolGuard {
    fn teardown(self) -> Result<()> {
        let _ = util::run("umount", &[&format!("{MNT}/boot/efi")]);
        util::run("zpool", &["export", POOL])
    }
}

fn zfs_layout(dev: &str, log: &dyn Fn(&str)) -> Result<PoolGuard> {
    log(&format!("Creating zpool {POOL}"));
    util::run("zpool", &[
        "create", "-f", "-o", &format!("altroot={MNT}"),
        "-O", "compress=lz4", "-O", "atime=off", "-O", "mountpoint=none",
        POOL, &format!("{dev}p2"),
    ])?;

    // Standard FreeBSD-on-ZFS dataset layout (bsdinstall's).
    for args in [
        vec!["create", "-o", "mountpoint=none", "zroot/ROOT"],
        vec!["create", "-o", "mountpoint=/", "zroot/ROOT/default"],
        vec!["create", "-o", "mountpoint=/tmp", "-o", "exec=on", "-o", "setuid=off", "zroot/tmp"],
        vec!["create", "-o", "mountpoint=/usr", "-o", "canmount=off", "zroot/usr"],
        vec!["create", "zroot/usr/home"],
        vec!["create", "-o", "mountpoint=/var", "-o", "canmount=off", "zroot/var"],
        vec!["create", "-o", "exec=off", "-o", "setuid=off", "zroot/var/audit"],
        vec!["create", "-o", "exec=off", "-o", "setuid=off", "zroot/var/crash"],
        vec!["create", "-o", "exec=off", "-o", "setuid=off", "zroot/var/log"],
        vec!["create", "-o", "atime=on", "zroot/var/mail"],
        vec!["create", "-o", "exec=on", "-o", "setuid=off", "zroot/var/tmp"],
    ] {
        util::run("zfs", &args)?;
    }
    util::run("zpool", &["set", "bootfs=zroot/ROOT/default", POOL])?;
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
    if Path::new(MEDIA_DIST).is_dir() {
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
        "zfs_load=\"YES\"\nvfs.root.mountfrom=\"zfs:zroot/ROOT/default\"\n\
         kern.geom.label.disk_ident.enable=\"0\"\nkern.geom.label.gptid.enable=\"0\"\n",
    )?;

    // fstab: ESP for future loader updates.
    fs::write(
        format!("{MNT}/etc/fstab"),
        format!("/dev/gpt/efiboot\t/boot/efi\tmsdosfs\trw\t\t2\t2\n"),
    )?;
    Ok(())
}

fn system_files(hostname: &str) -> Result<()> {
    fs::write(
        format!("{MNT}/etc/rc.conf"),
        format!(
            "hostname=\"{hostname}\"\nzfs_enable=\"YES\"\nsshd_enable=\"YES\"\n\
             ifconfig_DEFAULT=\"SYNCDHCP\"\nfreearchy_firstboot_enable=\"YES\"\n"
        ),
    )?;
    Ok(())
}

fn create_user(username: &str, password: Option<&str>, log: &dyn Fn(&str)) -> Result<()> {
    log(&format!("Creating user {username}"));
    util::run("chroot", &[
        MNT, "pw", "useradd", "-n", username, "-m", "-s", "/bin/sh",
        "-G", "wheel,video", "-c", "freearchy user",
    ])?;
    if let Some(pass) = password {
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

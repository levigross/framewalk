# kernel-debug.nix — QEMU-bootable debug kernel for framewalk kernel debugging.
#
# Builds:
#   1. A Linux kernel with debug symbols, LKDTM, KASAN, lockdep, and
#      syzbot-compatible features (namespaces, KCOV, fault injection)
#   2. A minimal initramfs (busybox + init script + 9p mount support)
#   3. A launch script that starts QEMU with GDB stub on :1234 and
#      virtfs host directory sharing
#
# Usage:
#   nix build .#kernel-debug-vm
#   ./result/bin/kernel-debug-vm                              # basic boot
#   ./result/bin/kernel-debug-vm --share ./bugs/my-bug        # share a dir with guest
#
# Connect framewalk (QEMU's gdbstub is all-stop only, so --no-non-stop):
#   framewalk-mcp --no-non-stop
#   (mi "-target-select remote localhost:1234")
#   (mi "-file-symbol-file /nix/store/.../vmlinux")
#   (set-hw-breakpoint "start_kernel")
#   (cont-and-wait 120)
#
# Syzbot workflow:
#   scripts/syzbot-fetch <extid>                              # download bug artifacts
#   ./result/bin/kernel-debug-vm --share ./bugs/<extid>       # boot with shared dir
#   # In VM: /shared/repro                                    # run the reproducer

{ pkgs, ... }:
let
  lib = pkgs.lib;

  # ── Debug kernel ────────────────────────────────────────────────────
  #
  # Based on linux_latest with a superset config that covers:
  # - Full debug symbols for GDB
  # - LKDTM kernel crash provocation
  # - KASAN memory error detection
  # - Lockdep deadlock detection
  # - Syzbot-compatible features (namespaces, KCOV, etc.)
  # - 9P filesystem for host directory sharing
  debugKernel = (pkgs.linux_latest.override {
    structuredExtraConfig = with lib.kernel; {
      # ── Debug symbols & GDB ──────────────────────────────────────
      DEBUG_KERNEL = yes;
      DEBUG_INFO = yes;
      DEBUG_INFO_DWARF_TOOLCHAIN_DEFAULT = yes;
      GDB_SCRIPTS = yes;
      KALLSYMS = yes;
      KALLSYMS_ALL = yes;

      # Disable KASLR so vmlinux symbol addresses match runtime
      RANDOMIZE_BASE = lib.mkForce no;

      # ── Crash provocation (LKDTM) ───────────────────────────────
      # Optional: nixpkgs's resolver occasionally drops LKDTM after a
      # kernel bump (deps shift in lib/Kconfig.debug). When that happens
      # we get a warning instead of a build failure, and the init script
      # at the bottom of this file already detects /sys/kernel/debug/
      # provoke-crash being absent. Reproducers from syzbot don't need
      # LKDTM — it's just a convenience for hand-triggered crashes.
      LKDTM = lib.kernel.option yes;

      # ── Memory error detection (KASAN) ───────────────────────────
      KASAN = yes;
      KASAN_GENERIC = yes;

      # ── Lock debugging ───────────────────────────────────────────
      LOCKDEP = yes;
      PROVE_LOCKING = yes;
      PROVE_RCU = yes;
      DEBUG_ATOMIC_SLEEP = yes;

      # ── Fault injection ──────────────────────────────────────────
      FAULT_INJECTION = yes;
      FAILSLAB = yes;
      FAULT_INJECTION_DEBUG_FS = yes;

      # ── KCOV (syzbot coverage-guided fuzzing support) ────────────
      KCOV = yes;
      KCOV_INSTRUMENT_ALL = yes;
      KCOV_ENABLE_COMPARISONS = yes;

      # ── Namespaces (required by syzbot reproducers) ──────────────
      NAMESPACES = yes;
      USER_NS = yes;
      UTS_NS = yes;
      IPC_NS = yes;
      PID_NS = yes;
      NET_NS = yes;
      CGROUP_PIDS = yes;
      MEMCG = yes;

      # ── Networking (for syzbot reproducers that use TUN/TAP) ─────
      TUN = yes;
      VETH = yes;
      BRIDGE = yes;
      NETFILTER = yes;
      NF_CONNTRACK = yes;
      NF_NAT = yes;
      NF_TABLES = yes;
      NF_TABLES_INET = yes;
      NETFILTER_XTABLES = yes;
      IP_NF_IPTABLES = yes;
      IP6_NF_IPTABLES = yes;

      # ── QEMU / virtio ───────────────────────────────────────────
      KVM_GUEST = yes;
      VIRTIO_PCI = yes;
      VIRTIO_BLK = yes;
      VIRTIO_NET = yes;
      VIRTIO_CONSOLE = yes;

      # ── 9P filesystem (host directory sharing via virtfs) ────────
      NET_9P = yes;
      NET_9P_VIRTIO = yes;
      "9P_FS" = yes;
      "9P_FS_POSIX_ACL" = yes;

      # ── Serial console ──────────────────────────────────────────
      SERIAL_8250 = yes;
      SERIAL_8250_CONSOLE = yes;

      # ── Essentials ──────────────────────────────────────────────
      DEVTMPFS = yes;
      DEVTMPFS_MOUNT = yes;
      DEBUG_FS = yes;
      PROC_FS = yes;
      SYSFS = yes;
      TMPFS = yes;

      # ── Misc syzbot-useful features ─────────────────────────────
      CONFIGFS_FS = yes;
      SECURITYFS = yes;
      BPF_SYSCALL = yes;
      USERFAULTFD = yes;
      FUSE_FS = yes;
    };
  }).overrideAttrs {
    dontStrip = true;
  };

  # ── Init script ────────────────────────────────────────────────────
  initScript = pkgs.writeScript "init" ''
    #!/bin/sh
    export PATH=/bin:/sbin:/usr/bin:/usr/sbin

    mount -t proc proc /proc
    mount -t sysfs sys /sys
    mount -t devtmpfs dev /dev
    mkdir -p /sys/kernel/debug
    mount -t debugfs debugfs /sys/kernel/debug
    mount -t tmpfs tmpfs /tmp

    # LKDTM is built-in — check if debugfs interface is available
    if [ -d /sys/kernel/debug/provoke-crash ]; then
        echo "[ok] lkdtm available"
    else
        echo "[--] lkdtm not available (debugfs may not be mounted)"
    fi

    # Mount 9p shared directory if available
    mkdir -p /shared
    if mount -t 9p -o trans=virtio,version=9p2000.L shared /shared 2>/dev/null; then
        echo "[ok] /shared mounted (host directory)"
        # List available reproducers
        if ls /shared/*.c /shared/repro 2>/dev/null | head -5; then
            echo "     ^^^ reproducers available"
        fi
    else
        echo "[--] /shared not mounted (no --share flag)"
    fi

    # Set up networking (loopback + basic ifconfig)
    ip link set lo up 2>/dev/null || ifconfig lo up 2>/dev/null || true

    echo ""
    echo "======================================"
    echo "  framewalk kernel debug VM"
    echo "======================================"
    echo ""
    echo "GDB:     tcp::1234"
    echo "vmlinux: ${debugKernel.dev}/vmlinux"
    echo ""
    echo "LKDTM:   echo TYPE > /sys/kernel/debug/provoke-crash/DIRECT"
    echo "  Types: PANIC BUG EXCEPTION OVERFLOW CORRUPT_STACK"
    echo "         WRITE_AFTER_FREE READ_AFTER_FREE SLAB_FREE_DOUBLE"
    echo ""
    if [ -d /shared ]; then
        echo "Shared:  /shared (host directory)"
        echo "  Run:   /shared/repro"
        echo ""
    fi

    exec /bin/sh
  '';

  # ── Initramfs ──────────────────────────────────────────────────────
  busybox = pkgs.pkgsStatic.busybox;

  initramfs = pkgs.runCommand "framewalk-initramfs" {
    nativeBuildInputs = [ pkgs.cpio pkgs.gzip ];
  } ''
    mkdir -p rootfs/{bin,sbin,proc,sys,dev,tmp,shared}

    # Busybox and symlinks
    cp ${busybox}/bin/busybox rootfs/bin/busybox
    chmod +x rootfs/bin/busybox
    for cmd in sh ls cat echo mount umount mkdir modprobe insmod rmmod \
               dmesg grep head tail sleep kill ps top \
               chmod chown cp mv rm ln wc tr ip ifconfig \
               vi less more sort uniq; do
      ln -sf busybox rootfs/bin/$cmd
    done
    ln -sf /bin/busybox rootfs/sbin/modprobe
    ln -sf /bin/busybox rootfs/sbin/ip

    # Init script
    cp ${initScript} rootfs/init
    chmod +x rootfs/init

    # Build cpio archive
    mkdir -p $out
    (cd rootfs && find . | cpio -o -H newc | gzip -9 > $out/initrd.gz)
  '';

  # ── Kernel image path ─────────────────────────────────────────────
  bzImageTarget = pkgs.stdenv.hostPlatform.linux-kernel.target or "bzImage";

  # ── QEMU launch script ────────────────────────────────────────────
  launchScript = pkgs.writeShellScriptBin "kernel-debug-vm" ''
    set -euo pipefail

    SHARE_DIR=""
    EXTRA_QEMU_ARGS=()
    PAUSE=true

    usage() {
        echo "Usage: kernel-debug-vm [OPTIONS] [-- QEMU_ARGS...]"
        echo ""
        echo "Options:"
        echo "  --share DIR     Share a host directory with the guest at /shared"
        echo "  --no-pause      Don't freeze CPU at startup (skip -S flag)"
        echo "  --help          Show this help"
        echo ""
        echo "Examples:"
        echo "  kernel-debug-vm                          # basic boot, wait for GDB"
        echo "  kernel-debug-vm --share ./bugs/my-bug    # share reproducer dir"
        echo "  kernel-debug-vm --no-pause               # boot immediately"
    }

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --share)
                SHARE_DIR="$(realpath "$2")"
                shift 2
                ;;
            --no-pause)
                PAUSE=false
                shift
                ;;
            --help|-h)
                usage
                exit 0
                ;;
            --)
                shift
                EXTRA_QEMU_ARGS+=("$@")
                break
                ;;
            *)
                EXTRA_QEMU_ARGS+=("$1")
                shift
                ;;
        esac
    done

    echo "framewalk kernel debug VM"
    echo "========================="
    echo ""
    echo "Kernel:  ${debugKernel}/${bzImageTarget}"
    echo "vmlinux: ${debugKernel.dev}/vmlinux"
    echo ""
    echo "GDB stub: tcp::1234"
    echo ""
    echo "Framewalk (QEMU is all-stop — use --no-non-stop):"
    echo "  framewalk-mcp --no-non-stop"
    echo "  (mi \"-target-select remote localhost:1234\")"
    echo "  (mi \"-file-symbol-file ${debugKernel.dev}/vmlinux\")"
    echo "  (set-hw-breakpoint \"start_kernel\")"
    echo ""

    QEMU_ARGS=(
        -kernel "${debugKernel}/${bzImageTarget}"
        -initrd "${initramfs}/initrd.gz"
        -append "console=ttyS0 nokaslr"
        -nographic
        -m 2G
        -smp 2
        -enable-kvm
        -s
    )

    if $PAUSE; then
        QEMU_ARGS+=(-S)
        echo "CPU paused — attach GDB and 'continue' to boot."
    else
        echo "Booting immediately (--no-pause)."
    fi

    # Add virtfs host directory sharing if requested
    if [[ -n "$SHARE_DIR" ]]; then
        echo "Sharing: $SHARE_DIR -> /shared (in guest)"
        QEMU_ARGS+=(
            -virtfs "local,path=$SHARE_DIR,mount_tag=shared,security_model=none,id=shared0"
        )
    fi

    echo ""
    echo "Press Ctrl-A X to exit QEMU."
    echo ""

    exec ${pkgs.qemu}/bin/qemu-system-x86_64 \
        "''${QEMU_ARGS[@]}" \
        "''${EXTRA_QEMU_ARGS[@]}"
  '';

in
{
  kernel-debug-vm = launchScript;
  kernel-debug-kernel = debugKernel;
}

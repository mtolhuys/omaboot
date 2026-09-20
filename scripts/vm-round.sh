#!/bin/bash
# The M1 proof in a disposable guest: install the package, apply a theme,
# reboot, look at the three screens; reset, reboot, prove the guest is stock;
# apply again, kill -9 omaboot during the initramfs step, reboot, revert.
# The LUKS unlock prompt is proven the way Lock Screen Explorer's
# extras/boot-vm-test.sh does it: the guest's own kernel and initramfs, as the
# apply rebuilt them, booted against a throwaway encrypted disk.
#
# Nothing on the host is touched: the guest is a qcow2 overlay over the
# omarchy-iso lab's base image (an installed, unencrypted Omarchy 4.0.3 with
# the lab's SSH key), driven over QMP (screendump, send-key) and SSH, the
# way bin/omarchy-iso-test in that repository does it, with one difference:
# no serial port. With a UART present Limine appends `console=uart,io,0x3f8
# console=tty0` to the kernel command line, and Plymouth, seeing a serial
# console, forces its details splash ("serial consoles detected, managing
# them with details forced" in its debug log), so no theme, omaboot's or
# Omarchy's, is ever drawn in the lab's guest. Without the port the guest
# boots as a laptop does.
#
#   scripts/vm-round.sh <package.pkg.tar.zst> <theme dir> <evidence dir>
#
# Or source it and run the phases by hand, in the order main() does.
set -euo pipefail

LAB=${OMARCHY_ISO_LAB:-$HOME/Projects/omarchy/omarchy-iso/test-runs/omarchy-4.0.3}
BASE_DISK="$LAB/base.qcow2"
SSH_KEY="$LAB/id_ed25519"
GUEST_USER=omarchy
GUEST_PASSWORD=omarchy
SSH_PORT=${VM_SSH_PORT:-2244}
MEMORY=${VM_MEMORY:-4096}
OVMF_CODE=/usr/share/edk2/x64/OVMF_CODE.4m.fd

PKG=${1:-}
THEME_DIR=${2:-}
OUT=${3:-}
WORK=${VM_WORK:-${OUT:-/tmp/omaboot-vm}/work}
QMP_SOCK=${QMP_SOCK:-/tmp/omaboot-vm-qmp.sock}
PIDFILE="$WORK/qemu.pid"
LOG="$OUT/commands.log"

log() { printf '\033[1;35m==> %s\033[0m\n' "$1"; printf '\n== %s\n' "$1" >>"$LOG"; }
# Every command the evidence quotes goes through here, with its output.
run() { printf '$ %s\n' "$*" >>"$LOG"; "$@" 2>&1 | tee -a "$LOG"; return "${PIPESTATUS[0]}"; }
guest() { printf 'guest$ %s\n' "$*" >>"$LOG"; ssh_guest "$@" 2>&1 | tee -a "$LOG"; return "${PIPESTATUS[0]}"; }

# The guest and the unlock VM each have a QMP socket of their own; the one
# the helpers talk to is $QMP_SOCK, swapped by unlock_vm_start and back by
# unlock_vm_stop.
GUEST_QMP_SOCK="$QMP_SOCK"
qmp() {
  printf '{"execute":"qmp_capabilities"}\n{"execute":%s}\n' "$1" |
    timeout 5 socat -t 2 - "UNIX-CONNECT:$QMP_SOCK" 2>/dev/null || true
}
screendump() { qmp "\"screendump\", \"arguments\": {\"filename\": \"$1\"}" >/dev/null; }
shot() {
  local ppm="$WORK/.shot.ppm"
  screendump "$ppm"
  [[ -s $ppm ]] || return 1
  magick "$ppm" "$OUT/$1.png" && rm -f "$ppm"
  echo "shot $1.png" >>"$LOG"
}
press() {
  local part json="" parts
  IFS='-' read -ra parts <<<"$1"
  for part in "${parts[@]}"; do json+="{\"type\":\"qcode\",\"data\":\"$part\"},"; done
  qmp "\"send-key\", \"arguments\": {\"keys\": [${json%,}]}" >/dev/null
}
type_text() {
  local text="$1" ch i
  for ((i = 0; i < ${#text}; i++)); do
    ch=${text:i:1}
    case "$ch" in
      [a-z0-9]) press "$ch" ;;
      [A-Z]) press "shift-${ch,,}" ;;
      " ") press spc ;;
      *) echo "type_text: unsupported: $ch" >&2; return 1 ;;
    esac
    sleep 0.05
  done
}

vm_running() { [[ -f $PIDFILE ]] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; }

start_vm() {
  mkdir -p "$WORK" "$OUT"
  if [[ ! -f $WORK/run.qcow2 ]]; then
    run qemu-img create -f qcow2 -b "$BASE_DISK" -F qcow2 "$WORK/run.qcow2"
    cp "$LAB/OVMF_VARS.4m.fd" "$WORK/OVMF_VARS.4m.fd"
  fi
  rm -f "$QMP_SOCK"
  qemu-system-x86_64 \
    -cpu host -enable-kvm -machine q35,accel=kvm \
    -smp 4 -m "$MEMORY" \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
    -drive if=pflash,format=raw,file="$WORK/OVMF_VARS.4m.fd" \
    -drive file="$WORK/run.qcow2",format=qcow2,if=none,id=drive0 \
    -device virtio-blk-pci,drive=drive0,bootindex=1 \
    -device virtio-vga \
    -display none \
    -usb -device usb-tablet \
    -netdev user,id=net0,hostfwd=tcp:127.0.0.1:$SSH_PORT-:22 \
    -device virtio-net-pci,netdev=net0 \
    -qmp "unix:$QMP_SOCK,server,nowait" \
    -serial "${VM_SERIAL:-none}" \
    -pidfile "$PIDFILE" \
    -daemonize
  echo "qemu started, pid $(cat "$PIDFILE")" >>"$LOG"
}

ssh_guest() {
  ssh -i "$SSH_KEY" -p "$SSH_PORT" -o BatchMode=yes -o IdentitiesOnly=yes \
    -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
    -o ConnectTimeout=5 -o LogLevel=ERROR "$GUEST_USER@127.0.0.1" "$@"
}
scp_guest() {
  scp -i "$SSH_KEY" -P "$SSH_PORT" -o BatchMode=yes -o IdentitiesOnly=yes \
    -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -q "$@"
}
sudo_guest() { ssh_guest "echo '$GUEST_PASSWORD' | sudo -S -p '' $*"; }

wait_for_ssh() {
  local timeout=${1:-300} waited=0
  until ssh_guest true 2>/dev/null; do
    vm_running || { echo "the VM is gone" >&2; return 1; }
    ((waited >= timeout)) && { echo "no SSH after ${timeout}s" >&2; return 1; }
    sleep 3; ((waited += 3))
  done
}
wait_for_down() {
  local waited=0
  while vm_running && ((waited < 120)); do sleep 1; ((waited += 1)); done
  vm_running && return 1 || return 0
}

stop_vm() {
  vm_running || return 0
  sudo_guest systemctl poweroff >/dev/null 2>&1 || qmp '"system_powerdown"' >/dev/null
  wait_for_down || { qmp '"quit"' >/dev/null; sleep 1; }
  vm_running && kill "$(cat "$PIDFILE")" 2>/dev/null || true
}

session_started() {
  ssh_guest "pgrep -u \$(id -u) -x Hyprland >/dev/null" 2>/dev/null
}
# Log in at the greeter the way a person does: SDDM remembers the user and
# focuses the password field, so it is the password and Return.
login_at_greeter() {
  local name="$1" waited=0
  until session_started; do
    ((waited == 0)) && shot "$name-greeter"
    type_text "$GUEST_PASSWORD"
    ((waited == 0)) && shot "$name-greeter-typed"
    press ret
    sleep 10; ((waited += 10))
    ((waited >= 180)) && { shot "$name-login-timeout"; echo "no session after 180s" >&2; return 1; }
  done
  sleep 5
  shot "$name-desktop"
}
# Run in the user's graphical session: the shell's IPC needs the runtime dir,
# the Hyprland instance and OMARCHY_PATH, which the login session exports and
# an SSH session does not (omarchy-shell exits 1 with "OMARCHY_PATH is not
# set" without it).
session() {
  ssh_guest "export XDG_RUNTIME_DIR=/run/user/\$(id -u) OMARCHY_PATH=/usr/share/omarchy; \
    export HYPRLAND_INSTANCE_SIGNATURE=\$(ls -t \$XDG_RUNTIME_DIR/hypr 2>/dev/null | head -1); \
    export WAYLAND_DISPLAY=\$(ls \$XDG_RUNTIME_DIR | grep -m1 '^wayland-[0-9]*\$'); $*"
}

phase_install() {
  log "install the package and the theme in the guest"
  scp_guest "$PKG" "$GUEST_USER@127.0.0.1:/tmp/omaboot.pkg.tar.zst"
  guest "echo '$GUEST_PASSWORD' | sudo -S -p '' pacman -U --noconfirm /tmp/omaboot.pkg.tar.zst"
  guest "pacman -Q omaboot-git; ls -la /usr/bin/omaboot /usr/bin/omaboot-apply /usr/share/omaboot/plugin; omaboot --version 2>&1 || true; omaboot-apply protocol"
  guest "mkdir -p ~/.config/omaboot/themes"
  scp_guest -r "$THEME_DIR" "$GUEST_USER@127.0.0.1:.config/omaboot/themes/matte"
  guest "cat ~/.config/omaboot/themes/matte/theme.toml"
  log "the plugin, from the packaged copy, into the running shell"
  session "omaboot plugin install"
  guest "ls -la ~/.config/omarchy/plugins/ | grep omaboot; readlink ~/.config/omarchy/plugins/mtolhuys.omaboot"
  session "omarchy-shell shell plugins 2>&1 | grep -i omaboot || true"
  log "what boots now, before anything is applied"
  guest "omaboot status"
  guest "cat /etc/plymouth/plymouthd.conf; ls /etc/sddm.conf.d; ls -d /usr/share/plymouth/themes/* /usr/share/sddm/themes/*; ls ~/.local/state/omaboot 2>&1 || true"
  guest "echo '$GUEST_PASSWORD' | sudo -S -p '' sha256sum /boot/EFI/Linux/omarchy_linux.efi" | tee "$OUT/uki-stock.sha256"
}

phase_apply() {
  local name="$1"
  log "omaboot apply matte ($name)"
  guest "echo '$GUEST_PASSWORD' | omaboot --password-stdin apply matte" | tee "$OUT/apply-$name.txt"
  guest "omaboot status"
  guest "cat /etc/plymouth/plymouthd.conf; ls /etc/sddm.conf.d; cat /etc/sddm.conf.d/zz-omaboot.conf; ls /usr/share/plymouth/themes/omaboot /usr/share/sddm/themes/omaboot; ls ~/.local/state/omaboot; cat ~/.local/state/omaboot/*.json 2>/dev/null || true"
  guest "echo '$GUEST_PASSWORD' | sudo -S -p '' sha256sum /boot/EFI/Linux/omarchy_linux.efi" | tee "$OUT/uki-$name.sha256"
}

# The guest boots and shuts down in a few seconds, so one picture a second
# misses the splash. Sample the screen as fast as QMP answers for a while,
# then keep one PNG per distinct frame, numbered in order.
sample() {
  local name="$1" seconds="$2" dir="$WORK/sample-$name" i=0 last="" sum ppm
  rm -rf "$dir"; mkdir -p "$dir"
  local end=$((SECONDS + seconds))
  while ((SECONDS < end)); do
    ppm=$(printf '%s/%04d.ppm' "$dir" "$i")
    screendump "$ppm"
    ((i += 1))
    sleep 0.15
  done
  i=0
  for ppm in "$dir"/*.ppm; do
    [[ -s $ppm ]] || continue
    sum=$(md5sum <"$ppm")
    if [[ $sum != "$last" ]]; then
      magick "$ppm" "$OUT/$name-$(printf '%02d' "$i").png"
      ((i += 1))
      last=$sum
    fi
  done
  rm -rf "$dir"
  echo "sampled $name: $i distinct frames" | tee -a "$LOG"
}

# A transient unit whose stop takes a while, so the shutdown splash stays up
# long enough to be seen. It is gone with the reboot.
hold_shutdown() {
  sudo_guest "systemd-run --unit=omaboot-hold -p 'ExecStop=/usr/bin/sleep 12' -p TimeoutStopSec=30 -p KillMode=none /usr/bin/sleep infinity"
}

# The unlock prompt, Lock Screen Explorer's way (extras/boot-vm-test.sh in
# that plugin): the guest's own kernel and initramfs, taken out of the UKI
# the apply rebuilt, booted against a throwaway LUKS disk with the kernel
# told the root is on it. Nothing is overlaid: the initramfs carries the
# omaboot theme and plymouthd.conf exactly as the apply left them. A tiny
# root with a do-nothing init keeps the boot from reaching an emergency
# shell after the unlock. No serial port here either, for the reason above.
fetch_uki() {
  local name="$1"
  sudo_guest "sh -c 'cp /boot/EFI/Linux/omarchy_linux.efi /tmp/uki.efi && chown $GUEST_USER /tmp/uki.efi'" >/dev/null
  scp_guest "$GUEST_USER@127.0.0.1:/tmp/uki.efi" "$WORK/uki-$name.efi"
  sha256sum "$WORK/uki-$name.efi" | tee -a "$LOG"
}
unlock_vm_start() {
  local name="$1" gpu="${2:-virtio-vga}"
  local u="$WORK/unlock-$name"
  mkdir -p "$u"
  objcopy --dump-section .linux="$u/vmlinuz" --dump-section .initrd="$u/initrd.img" "$WORK/uki-$name.efi" /dev/null 2>/dev/null || true
  [[ -s $u/vmlinuz && -s $u/initrd.img ]] || { echo "the UKI gave no kernel or initrd" >&2; return 1; }
  if [[ ! -f $WORK/luks.img ]]; then
    truncate -s 64M "$WORK/luks.img"
    echo -n omarchy | cryptsetup luksFormat --type luks2 --pbkdf pbkdf2 --pbkdf-force-iterations 1000 -q "$WORK/luks.img" -
  fi
  if [[ ! -f $WORK/rootfs.img ]]; then
    printf '#include <unistd.h>\nint main(void){for(;;)pause();}\n' > "$WORK/pause.c"
    gcc -static -Os -o "$WORK/pause-init" "$WORK/pause.c"
    mkdir -p "$WORK/rootfs/sbin" && cp "$WORK/pause-init" "$WORK/rootfs/sbin/init"
    truncate -s 16M "$WORK/rootfs.img"
    mkfs.ext4 -q -d "$WORK/rootfs" "$WORK/rootfs.img"
  fi
  cp /usr/share/edk2/x64/OVMF_VARS.4m.fd "$u/ovmf_vars.fd"
  QMP_SOCK="$GUEST_QMP_SOCK-unlock"
  rm -f "$QMP_SOCK"
  qemu-system-x86_64 -enable-kvm -cpu host -m 2048 \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
    -drive if=pflash,format=raw,file="$u/ovmf_vars.fd" \
    -kernel "$u/vmlinuz" -initrd "$u/initrd.img" \
    -append "cryptdevice=/dev/vda:root root=/dev/vdb rw quiet splash loglevel=0 systemd.show_status=false rd.udev.log_level=0 vt.global_cursor_default=0" \
    -drive file="$WORK/luks.img",format=raw,if=virtio \
    -drive file="$WORK/rootfs.img",format=raw,if=virtio \
    -device "$gpu" -display none -usb -device usb-tablet \
    -qmp "unix:$QMP_SOCK,server,nowait" \
    -serial none -no-reboot -pidfile "$u/qemu.pid" -daemonize
  echo "unlock vm started ($gpu), pid $(cat "$u/qemu.pid")" | tee -a "$LOG"
}
unlock_vm_stop() {
  local u="$WORK/unlock-$1"
  [[ -f $u/qemu.pid ]] && kill "$(cat "$u/qemu.pid")" 2>/dev/null || true
  QMP_SOCK="$GUEST_QMP_SOCK"
}

# Everything a stock guest looks like at the places omaboot touches, in one
# listing, so two runs of it can be compared line for line.
stock_fingerprint() {
  ssh_guest "echo '== plymouthd.conf'; cat /etc/plymouth/plymouthd.conf; \
    echo '== sddm.conf.d'; ls /etc/sddm.conf.d; \
    echo '== drop-in'; cat /etc/sddm.conf.d/zz-omaboot.conf 2>&1; \
    echo '== plymouth themes'; ls /usr/share/plymouth/themes; \
    echo '== sddm themes'; ls /usr/share/sddm/themes; \
    echo '== omaboot dirs'; ls -d /usr/share/plymouth/themes/omaboot /usr/share/sddm/themes/omaboot ~/.local/state/omaboot 2>&1; \
    echo '== omarchy theme untouched'; (cd /usr/share/plymouth/themes/omarchy && find . -type f -exec sha256sum {} + | sort | sha256sum); (cd /usr/share/sddm/themes/omarchy && find . -type f -exec sha256sum {} + | sort | sha256sum); \
    echo '== initramfs'; echo '$GUEST_PASSWORD' | sudo -S -p '' lsinitcpio /boot/EFI/Linux/omarchy_linux.efi 2>/dev/null | grep -E 'plymouth/themes|plymouthd.conf' | sort; \
    echo '== plymouthd.conf in the initramfs'; echo '$GUEST_PASSWORD' | sudo -S -p '' sh -c 'cd /tmp && rm -rf ply-x && mkdir ply-x && cd ply-x && lsinitcpio -x /boot/EFI/Linux/omarchy_linux.efi >/dev/null 2>&1; cat /tmp/ply-x/etc/plymouth/plymouthd.conf 2>&1; rm -rf /tmp/ply-x'"
}

phase_reset() {
  log "omaboot reset"
  guest "echo '$GUEST_PASSWORD' | omaboot --password-stdin reset" | tee "$OUT/reset.txt"
  guest "omaboot status"
  log "the guest after reset, against the stock listing"
  stock_fingerprint | tee "$OUT/fingerprint-after-reset.txt"
  guest "echo '$GUEST_PASSWORD' | sudo -S -p '' sha256sum /boot/EFI/Linux/omarchy_linux.efi" | tee "$OUT/uki-after-reset.sha256"
}

# Apply again and kill -9 the engine the moment it reports the initramfs
# step started, then reboot: the guest must come up, and revert must put the
# rollback point back. The watcher runs inside the guest, reading the
# engine's --json stream, so the kill lands within a tenth of a second.
phase_kill() {
  log "apply matte with kill -9 during the initramfs step"
  guest "cat > /tmp/kill-during-initramfs.sh <<'EOS'
#!/bin/bash
echo omarchy | setsid omaboot --json --password-stdin apply matte > /tmp/apply-killed.jsonl 2>&1 &
pid=\$!
for i in \$(seq 1 3000); do
  if grep -q '\"state\":\"started\",\"step\":\"initramfs\"' /tmp/apply-killed.jsonl 2>/dev/null; then
    ps -o pid,ppid,etimes,args -p \$pid
    pgrep -a -f 'limine-mkinitcpio|mkinitcpio' || true
    kill -9 \$pid && echo \"killed omaboot pid \$pid during the initramfs step\"
    break
  fi
  sleep 0.1
done
wait \$pid 2>/dev/null; echo \"omaboot exit status \$?\"
sleep 1; pgrep -a -f 'limine-mkinitcpio|mkinitcpio' || echo 'no mkinitcpio running any more'
EOS
chmod +x /tmp/kill-during-initramfs.sh && /tmp/kill-during-initramfs.sh"
  guest "cat /tmp/apply-killed.jsonl | grep -o '\"event\":\"step\"[^}]*' " | tee "$OUT/apply-killed-steps.txt"
  log "the guest right after the kill"
  guest "omaboot status; cat /etc/plymouth/plymouthd.conf; ls /etc/sddm.conf.d ~/.local/state/omaboot; cat ~/.local/state/omaboot/rollback.toml 2>&1" | tee "$OUT/after-kill.txt"
}

phase_revert() {
  log "omaboot revert after the interrupted apply"
  guest "echo '$GUEST_PASSWORD' | omaboot --password-stdin revert" | tee "$OUT/revert.txt"
  guest "omaboot status"
  stock_fingerprint | tee "$OUT/fingerprint-after-revert.txt"
  guest "echo '$GUEST_PASSWORD' | sudo -S -p '' sha256sum /boot/EFI/Linux/omarchy_linux.efi" | tee "$OUT/uki-after-revert.sha256"
}

# The whole round, in the order the evidence tells it. Sourcing the file
# defines the functions and runs nothing.
main() {
  [[ -n $PKG && -n $THEME_DIR && -n $OUT ]] || { sed -n 2,20p "$0"; exit 2; }
  mkdir -p "$OUT"
  start_vm; wait_for_ssh 300
  login_at_greeter 00-stock
  phase_install
  phase_apply first
  login_at_greeter 02-applied
  hold_shutdown; (sudo_guest systemctl reboot >/dev/null 2>&1 &); sample 03-applied-reboot 50
  wait_for_ssh 300; login_at_greeter 03-applied-after
  fetch_uki applied
  unlock_vm_start applied; sleep 10; shot 04-unlock-applied-prompt
  type_text omar; sleep 1; shot 04-unlock-applied-typed; type_text chy; press ret; sleep 4; shot 04-unlock-applied-unlocked
  unlock_vm_stop applied
  stock_fingerprint > "$OUT/fingerprint-applied.txt"
  phase_reset
  hold_shutdown; (sudo_guest systemctl reboot >/dev/null 2>&1 &); sample 05-reset-reboot 50
  wait_for_ssh 300; login_at_greeter 05-reset
  fetch_uki reset
  unlock_vm_start reset; sleep 10; shot 06-unlock-reset-prompt; unlock_vm_stop reset
  phase_kill
  hold_shutdown; (sudo_guest systemctl reboot >/dev/null 2>&1 &); sample 07-killed-reboot 50
  wait_for_ssh 300; login_at_greeter 07-killed
  phase_revert
  hold_shutdown; (sudo_guest systemctl reboot >/dev/null 2>&1 &); sample 08-reverted-reboot 50
  wait_for_ssh 300; login_at_greeter 08-reverted
  stop_vm
  log "done; pictures and logs in $OUT"
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  main
fi

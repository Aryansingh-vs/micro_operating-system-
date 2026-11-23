#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
./build.sh

VM_NAME="AryOS"
VDI="dist/aryos-bios.vdi"

VBoxManage unregistervm "$VM_NAME" --delete >/dev/null 2>&1 || true
rm -f "$VDI"

VBoxManage convertfromraw dist/aryos-bios.img "$VDI" --format VDI
VBoxManage createvm --name "$VM_NAME" --ostype "Other_64" --register
VBoxManage modifyvm "$VM_NAME" \
  --memory 128 \
  --cpus 1 \
  --firmware bios \
  --graphicscontroller vboxvga \
  --nic1 none
VBoxManage storagectl "$VM_NAME" --name "IDE" --add ide --controller PIIX4
VBoxManage storageattach "$VM_NAME" --storagectl "IDE" --port 0 --device 0 --type hdd --medium "$VDI"
VBoxManage startvm "$VM_NAME" --type gui

printf "VirtualBox VM '%s' started with 128 MB RAM and BIOS firmware.\n" "$VM_NAME"

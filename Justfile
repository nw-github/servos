initrd:
    cargo b
    mkdir -p initrd/bin

    rsync target/riscv64imac-unknown-none-elf/debug/init initrd/bin/init
    rsync target/riscv64imac-unknown-none-elf/debug/ls initrd/bin/ls
    rsync target/riscv64imac-unknown-none-elf/debug/sh initrd/bin/sh
    rsync target/riscv64imac-unknown-none-elf/debug/tests initrd/bin/tests
    rsync target/riscv64imac-unknown-none-elf/debug/bf initrd/bin/bf
    rsync target/riscv64imac-unknown-none-elf/debug/cat initrd/bin/cat
    rsync target/riscv64imac-unknown-none-elf/debug/shutdown initrd/bin/shutdown
    rsync target/riscv64imac-unknown-none-elf/debug/echo initrd/bin/echo
    rsync target/riscv64imac-unknown-none-elf/debug/kill initrd/bin/kill

    python mkfs.py initrd initrd.img

test: initrd
    cargo r --bin servos

debug-gdb:
    rust-gdb target/riscv64imac-unknown-none-elf/debug/servos

debug-qemu:
    qemu-system-riscv64 -s -S -machine virt -nographic -serial mon:stdio -kernel target/riscv64imac-unknown-none-elf/debug/servos

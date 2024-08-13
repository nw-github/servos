test:
    cargo build --bin init
    rsync target/riscv64imac-unknown-none-elf/debug/init test/init
    python mkfs.py test out.img
    cargo run --bin servos

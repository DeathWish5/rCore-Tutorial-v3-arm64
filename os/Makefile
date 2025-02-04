# Building
ARCH := aarch64
TARGET := $(shell cat ../targets.txt | grep $(ARCH))
MODE := release
KERNEL_ELF := target/$(TARGET)/$(MODE)/os
KERNEL_BIN := $(KERNEL_ELF).bin

BUILD_ARGS := --target $(TARGET) -Zbuild-std=core,alloc
ifeq ($(MODE), release)
  BUILD_ARGS += --release
endif

# BOARD
BOARD ?= qemu

# Binutils
OBJDUMP := rust-objdump --arch-name=$(ARCH) --print-imm-hex
OBJCOPY := rust-objcopy --binary-architecture=$(ARCH)

# Disassembly
DISASM ?= -d

# File image
FS_IMG := ../user/target/$(TARGET)/$(MODE)/fs.img
APP_SRC := ../user/src/bin/
APP_DIR := ../user/target/aarch64-unknown-none/$(MODE)/

fs-img:
	@cd ../user && make build
	@rm -f $(FS_IMG)
	@cd ../easy-fs-fuse && cargo run --release -- -s $(APP_SRC) -t $(APP_DIR)

# QEMU
QEMU := qemu-system-$(ARCH)
QEMU_ARGS := -nographic
ifeq ($(ARCH), aarch64)
  QEMU_ARGS += \
    -cpu cortex-a72 \
    -machine virt \
    -kernel $(KERNEL_BIN) \
	-drive file=$(FS_IMG),if=none,format=raw,id=x0 \
    -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0
endif

# GDB
GDB := gdb-multiarch

build: $(KERNEL_BIN) fs-img 

env:
	(rustup target list | grep "$(TARGET) (installed)") || rustup target add $(TARGET)
	cargo install cargo-binutils --vers =0.3.3
	rustup component add rust-src
	rustup component add llvm-tools-preview

$(KERNEL_BIN): kernel
	@$(OBJCOPY) $(KERNEL_ELF) --strip-all -O binary $@

kernel:
	@echo Arch: $(ARCH), Platform: $(BOARD)
	cargo build $(BUILD_ARGS)

clean:
	@cd ../user && make clean
	@cargo clean

disasm:
	@$(OBJDUMP) $(DISASM) $(KERNEL_ELF) > disasm.txt

run: run-inner

run-inner: build
	$(QEMU) $(QEMU_ARGS)


fmt:
	cd ../easy-fs; cargo fmt; cd ../easy-fs-fuse cargo fmt; cd ../os ; cargo fmt; cd ../user; cargo fmt; cd ../os

check:
	cargo fmt --all -- --check
	cargo check --target $(TARGET)
	cargo clippy --target $(TARGET)

debug: build
	@tmux new-session -d \
		"$(QEMU) $(QEMU_ARGS) -s -S" && \
		tmux split-window -h "$(GDB) $(KERNEL_ELF) -ex 'target remote localhost:1234'" && \
		tmux -2 attach-session -d

.PHONY: build env kernel clean disasm run run-inner debug

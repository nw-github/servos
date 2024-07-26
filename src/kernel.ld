OUTPUT_ARCH("riscv")
ENTRY(_start)

MEMORY {
  ram   (wxa) : ORIGIN = 0x80200000, LENGTH = 128M
}

PHDRS {
  text PT_LOAD;
  data PT_LOAD;
  bss PT_LOAD;
}

SECTIONS {
  . = ORIGIN(ram);
  PROVIDE(_text_start = .);     # ORIGIN is already page aligned

  .text : {
    *(.text.init)               # ensure _start is loaded at ORIGIN
    *(.text .text.*)
    PROVIDE(_text_end = .);
  } >ram

  PROVIDE(_global_pointer = .); # this is magic, google "linker relaxation"

  .rodata : {
    . = ALIGN(0x1000);          # page align RODATA for vm mapping
    PROVIDE(_rodata_start = .);
    *(.rodata .rodata.*)
    PROVIDE(_rodata_end = .);
  } >ram

  .data : {
    . = ALIGN(0x1000);          # page align DATA for vm mapping (bss & data are mapped together)
    PROVIDE(_data_start = .);
    *(.sdata .sdata.*) *(.data .data.*)
    PROVIDE(_data_end = .);
  } >ram

  .bss :{                       # finally, the BSS
    PROVIDE(_bss_start = .);
    *(.sbss .sbss.*) *(.bss .bss.*)
    PROVIDE(_bss_end = .);
  } >ram

  PROVIDE(_kernel_end = .);
}

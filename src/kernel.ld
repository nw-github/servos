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
  PROVIDE(_text_start = .);

  .text : {                     # put code first
    *(.text.init)
    *(.text .text.*)            # then put anything else in .text
    PROVIDE(_text_end = .);
  } >ram

  PROVIDE(_global_pointer = .); # this is magic, google "linker relaxation"

  .rodata : {                   # next, read-only data
    PROVIDE(_rodata_start = .);
    *(.rodata .rodata.*)
    PROVIDE(_rodata_end = .);
  } >ram

  .data : {                     # and the data section
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

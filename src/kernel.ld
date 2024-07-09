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

  .text : {                     # put code first
    *(.text.init)
    *(.text .text.*)            # then put anything else in .text
  } >ram

  PROVIDE(_global_pointer = .); # this is magic, google "linker relaxation"

  .rodata : {                   # next, read-only data
    *(.rodata .rodata.*)
  } >ram

  .data : {                     # and the data section
    *(.sdata .sdata.*) *(.data .data.*)
  } >ram

  .bss :{                       # finally, the BSS
    PROVIDE(_bss_start = .);
    *(.sbss .sbss.*) *(.bss .bss.*)
    PROVIDE(_bss_end = .);
  } >ram

  PROVIDE(_kernel_end = .);
}

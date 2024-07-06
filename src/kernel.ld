OUTPUT_ARCH("riscv")
ENTRY(_start)

MEMORY {
  ram   (wxa) : ORIGIN = 0x80000000, LENGTH = 128M
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
  } >ram AT>ram :text           # put this section into the text segment

  PROVIDE(_global_pointer = .); # this is magic, google "linker relaxation"

  .rodata : {                   # next, read-only data
    *(.rodata .rodata.*)
  } >ram AT>ram :text           # goes into the text segment as well (since instructions are generally read-only)

  .data : {                     # and the data section
    *(.sdata .sdata.*) *(.data .data.*)
  } >ram AT>ram :data           # this will go into the data segment

  .bss :{                       # finally, the BSS
    PROVIDE(_bss_start = .);    # define a variable for the start of this section
    *(.sbss .sbss.*) *(.bss .bss.*)
    PROVIDE(_bss_end = .);      # ... and one at the end
  } >ram AT>ram :bss            # and this goes into the bss segment

  PROVIDE(_kernel_end = .);
}

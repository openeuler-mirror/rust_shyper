ENTRY(_start)
TARGET(binary)
INPUT("vm0img")
OUTPUT_FORMAT(default)
SECTIONS
{
    . = TEXT_START;

	_image_start = ABSOLUTE(.);

    .boot : {
        *(.text.boot)
        *(.data.boot)
    }

    .text : {
        *(.text*)
    }

    .rodata : {
        *(.rodata*)
    }


    .data : {
        *(.data*)
        vm0img(.vm0image)
    }

    .bss (NOLOAD) : ALIGN(4096) {
        *(.bss.stack)

        . = ALIGN(4096);
        _bss_begin = .;
        *(.bss*)
        . = ALIGN(4096);
        _bss_end = .;
    }

    _image_end = ABSOLUTE(.);
}

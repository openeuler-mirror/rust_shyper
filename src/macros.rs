#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::utils::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! declare_enum_with_handler {
    (
        $enum_vis:vis enum $enum_name:ident [$array_vis:vis $array:ident => $handler_type:ty] {
            $($vis:vis $variant:ident => $handler:expr, )*
        }
    ) => {
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        #[repr(usize)]
        $enum_vis enum $enum_name {
            $($vis $variant, )*
        }
        $array_vis static $array: &[$handler_type] = &[
            $($handler, )*
        ];
    }
}

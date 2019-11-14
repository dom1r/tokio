pub(crate) trait One {
    fn one() -> Self;
}

macro_rules! one {
    ($($t:ident)*) => ($(
        impl One for $t { fn one() -> $t { 1 } }
    )*)
}

one! { i8 i16 i32 i64 isize u8 u16 u32 u64 usize }

pub(crate) trait Zero {
    fn zero() -> Self;
}

macro_rules! zero {
    ($($t:ident)*) => ($(
        impl Zero for $t { fn zero() -> $t { 0 } }
    )*)
}

zero! { i8 i16 i32 i64 isize u8 u16 u32 u64 usize }

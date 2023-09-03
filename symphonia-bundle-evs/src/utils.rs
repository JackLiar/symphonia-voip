pub unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    std::slice::from_raw_parts((p as *const T).cast::<u8>(), std::mem::size_of::<T>())
}

pub unsafe fn u8_slice_to_any<T: Sized>(p: &[u8]) -> &T {
    #[cfg(debug_assertions)]
    assert_eq!(p.len(), std::mem::size_of::<T>());
    &*(p.as_ptr().cast())
}

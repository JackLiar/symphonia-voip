pub fn struct_to_boxed_bytes<T>(s: T) -> Box<[u8]> {
    let size = std::mem::size_of::<T>();
    let mut vec = Vec::with_capacity(size);

    // 创建一个字节数组指针
    let ptr = &s as *const T as *const u8;

    // 将 struct 的内容复制到向量中
    unsafe {
        for i in 0..size {
            vec.push(ptr.add(i).read());
        }
    }

    // 将向量转换为 Box<[u8]>
    vec.into_boxed_slice()
}

pub fn bytes_to_struct<T>(b: &[u8]) -> T {
    assert!(b.len() == std::mem::size_of::<T>());
    let ptr = b.as_ptr() as *const T;
    unsafe { std::ptr::read_unaligned(ptr) }
}

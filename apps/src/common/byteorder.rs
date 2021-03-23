#[macro_export]
macro_rules! rte_cpu_to_be_16 {
    ($n:expr) => {
        (($n >> 8) & 0xFF) | (($n & 0xFF) << 8)
    };
}

pub fn parse_hex(src: &str) -> Result<u64, std::num::ParseIntError> {
    u64::from_str_radix(src, 16)
}

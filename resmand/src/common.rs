pub fn trim_str(original: &str) ->&str {
    original.trim_matches('\0').trim()
} 

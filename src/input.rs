pub struct Input {
    pub index: u8,
    pub label: Option<String>,
    pub gain: f32,
    pub pan: f32,
    pub muted: bool,
}

impl Input {
    pub fn new(index: u8) -> Input {
        Input {
            index,
            label: None,
            gain: 1.0,
            pan: 0.0,
            muted: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_input_defaults() {
        let input = Input::new(0);
        assert_eq!(input.index, 0);
        assert_eq!(input.label, None);
        assert_eq!(input.gain, 1.0);
        assert_eq!(input.pan, 0.0);
        assert_eq!(input.muted, false);
    }
}

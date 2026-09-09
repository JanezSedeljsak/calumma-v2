const MEOW: &[u8] = include_bytes!("../../sounds/meow.wav");

#[cfg(target_os = "macos")]
pub fn play() {
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSData;
    use std::cell::RefCell;

    thread_local! {
        static SOUND: RefCell<Option<Retained<NSSound>>> = const { RefCell::new(None) };
    }

    SOUND.with(|slot| {
        let mut slot = slot.borrow_mut();
        let sound = slot.get_or_insert_with(|| {
            let data = NSData::with_bytes(MEOW);
            NSSound::initWithData(NSSound::alloc(), &data).expect("decoding the meow")
        });
        sound.stop();
        sound.play();
    });
}

#[cfg(not(target_os = "macos"))]
pub fn play() {}

#[cfg(test)]
mod tests {
    use super::MEOW;

    #[test]
    fn the_meow_is_a_riff_wave_the_platform_can_decode() {
        assert_eq!(&MEOW[..4], b"RIFF");
        assert_eq!(&MEOW[8..12], b"WAVE");
        let declared = u32::from_le_bytes(MEOW[4..8].try_into().unwrap()) as usize;
        assert_eq!(declared + 8, MEOW.len());
    }
}

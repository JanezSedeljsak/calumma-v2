const MEOW: &[u8] = include_bytes!("../../sounds/meow.wav");

#[cfg(target_os = "macos")]
mod imp {
    use super::MEOW;
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSData;
    use std::cell::RefCell;

    thread_local! {
        static SOUND: RefCell<Option<Option<Retained<NSSound>>>> = const { RefCell::new(None) };
    }

    /// Decodes once and remembers the answer either way: a sound the system cannot decode is a
    /// silent easter egg, not a crash on the next click.
    fn with_sound(f: impl FnOnce(&Retained<NSSound>)) {
        SOUND.with(|slot| {
            let mut slot = slot.borrow_mut();
            let sound = slot.get_or_insert_with(|| {
                let data = NSData::with_bytes(MEOW);
                NSSound::initWithData(NSSound::alloc(), &data)
            });
            if let Some(sound) = sound {
                f(sound);
            }
        })
    }

    /// The real cost isn't decoding the wav, it's CoreAudio spinning up its
    /// engine on the very first `play()` in the process. Eat that hit here,
    /// muted, so the first real click is instant.
    pub fn preload() {
        with_sound(|sound| {
            let original_volume = sound.volume();
            sound.setVolume(0.0);
            sound.play();
            sound.stop();
            sound.setVolume(original_volume);
        });
    }

    pub fn play() {
        with_sound(|sound| {
            sound.stop();
            sound.play();
        });
    }
}

#[cfg(target_os = "macos")]
pub use imp::{play, preload};

#[cfg(not(target_os = "macos"))]
pub fn play() {}

#[cfg(not(target_os = "macos"))]
pub fn preload() {}

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

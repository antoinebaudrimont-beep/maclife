use crate::DynError;
use std::collections::HashSet;
use x11rb::protocol::xinput::{
    ConnectionExt as XInputConnectionExt, Device, DeviceType, EventMask, XIEventMask,
};
use x11rb::protocol::xproto::Window;
use x11rb::rust_connection::RustConnection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeviceRole {
    ToshyKeyboard,
    UserPointer,
    Ignore,
}

fn device_role(name: &[u8], type_: DeviceType, enabled: bool) -> DeviceRole {
    if !enabled {
        return DeviceRole::Ignore;
    }
    let name = String::from_utf8_lossy(name).to_ascii_lowercase();
    if type_ == DeviceType::SLAVE_KEYBOARD
        && name.contains("xwaykeyz")
        && name.contains("virtual")
        && name.contains("keyboard")
    {
        return DeviceRole::ToshyKeyboard;
    }
    if type_ == DeviceType::SLAVE_POINTER
        && !name.contains("xtest")
        && !name.contains("xwaykeyz")
    {
        return DeviceRole::UserPointer;
    }
    DeviceRole::Ignore
}

#[derive(Default, Debug)]
pub struct IntentDevices {
    toshy_keyboards: HashSet<u16>,
    user_pointers: HashSet<u16>,
}

impl IntentDevices {
    pub fn initialize(conn: &RustConnection, root: Window) -> Result<Self, DynError> {
        let version = conn.xinput_xi_query_version(2, 0)?.reply()?;
        if version.major_version < 2 {
            return Err("XInput2 is required for user-intent tracking".into());
        }
        let masks = [
            EventMask {
                deviceid: u16::from(Device::ALL_MASTER),
                mask: vec![
                    XIEventMask::RAW_KEY_PRESS
                        | XIEventMask::RAW_KEY_RELEASE
                        | XIEventMask::RAW_BUTTON_PRESS,
                ],
            },
            EventMask {
                deviceid: u16::from(Device::ALL),
                mask: vec![XIEventMask::HIERARCHY],
            },
        ];
        conn.xinput_xi_select_events(root, &masks)?.check()?;

        let mut devices = Self::default();
        devices.refresh(conn)?;
        Ok(devices)
    }

    pub fn refresh(&mut self, conn: &RustConnection) -> Result<(), DynError> {
        self.toshy_keyboards.clear();
        self.user_pointers.clear();
        let reply = conn
            .xinput_xi_query_device(u16::from(Device::ALL))?
            .reply()?;
        for info in reply.infos {
            match device_role(&info.name, info.type_, info.enabled) {
                DeviceRole::ToshyKeyboard => {
                    self.toshy_keyboards.insert(info.deviceid);
                }
                DeviceRole::UserPointer => {
                    self.user_pointers.insert(info.deviceid);
                }
                DeviceRole::Ignore => {}
            }
        }
        Ok(())
    }

    pub fn is_toshy_keyboard(&self, sourceid: u16) -> bool {
        self.toshy_keyboards.contains(&sourceid)
    }

    pub fn is_user_pointer(&self, sourceid: u16) -> bool {
        self.user_pointers.contains(&sourceid)
    }

    pub fn summary(&self) -> String {
        let mut keyboards: Vec<_> = self.toshy_keyboards.iter().copied().collect();
        let mut pointers: Vec<_> = self.user_pointers.iter().copied().collect();
        keyboards.sort_unstable();
        pointers.sort_unstable();
        format!("toshy_keyboards={keyboards:?} user_pointers={pointers:?}")
    }

    pub fn has_toshy_keyboard(&self) -> bool {
        !self.toshy_keyboards.is_empty()
    }
}

/// Toshy emits this key on the XWayKeyz keyboard immediately before and after
/// the dedicated F13/F14 lifecycle key. It is Control_R on the target XKB map.
pub const TOSHY_LIFECYCLE_WRAPPER_KEYCODE: u32 = 105;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyPhase {
    Press,
    Release,
}

impl KeyPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Press => "press",
            Self::Release => "release",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyIntent {
    User,
    LifecyclePrecursor,
    LifecycleClose,
    LifecycleQuit,
    LifecycleSuffix,
    Release,
}

impl KeyIntent {
    pub fn confirms_user_intent(self) -> bool {
        self == Self::User
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::LifecyclePrecursor => "lifecycle-precursor",
            Self::LifecycleClose => "lifecycle-close",
            Self::LifecycleQuit => "lifecycle-quit",
            Self::LifecycleSuffix => "lifecycle-suffix",
            Self::Release => "release",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ChordState {
    #[default]
    Idle,
    PrecursorDown,
    PrecursorReleased,
    LifecycleDown(u32),
    AwaitingSuffix,
    SuffixDown,
}

/// Classifies the observed XWayKeyz sequence without relying on a timer.
///
/// The physical trace is wrapper press/release, F13 or F14 press/release, then
/// wrapper press/release. The first wrapper event must be deferred until the
/// next key disambiguates it; otherwise it would incorrectly promote the
/// window xfwm4 focused after MacLife hid the previous application.
#[derive(Debug, Default)]
pub struct LifecycleChordTracker {
    state: ChordState,
}

impl LifecycleChordTracker {
    pub fn observe(
        &mut self,
        keycode: u32,
        phase: KeyPhase,
        close_keycode: u8,
        quit_keycode: u8,
    ) -> KeyIntent {
        let close = u32::from(close_keycode);
        let quit = u32::from(quit_keycode);
        let is_lifecycle = keycode == close || keycode == quit;
        let lifecycle_intent = || {
            if keycode == close {
                KeyIntent::LifecycleClose
            } else {
                KeyIntent::LifecycleQuit
            }
        };

        match (self.state, phase) {
            (ChordState::Idle, KeyPhase::Press)
                if keycode == TOSHY_LIFECYCLE_WRAPPER_KEYCODE =>
            {
                self.state = ChordState::PrecursorDown;
                KeyIntent::LifecyclePrecursor
            }
            (ChordState::PrecursorDown, KeyPhase::Release)
                if keycode == TOSHY_LIFECYCLE_WRAPPER_KEYCODE =>
            {
                self.state = ChordState::PrecursorReleased;
                KeyIntent::LifecyclePrecursor
            }
            (ChordState::PrecursorDown | ChordState::PrecursorReleased, KeyPhase::Press)
                if is_lifecycle =>
            {
                self.state = ChordState::LifecycleDown(keycode);
                lifecycle_intent()
            }
            (ChordState::LifecycleDown(active), KeyPhase::Release) if keycode == active => {
                self.state = ChordState::AwaitingSuffix;
                if keycode == close {
                    KeyIntent::LifecycleClose
                } else {
                    KeyIntent::LifecycleQuit
                }
            }
            (ChordState::LifecycleDown(_) | ChordState::AwaitingSuffix, KeyPhase::Press)
                if keycode == TOSHY_LIFECYCLE_WRAPPER_KEYCODE =>
            {
                self.state = ChordState::SuffixDown;
                KeyIntent::LifecycleSuffix
            }
            (ChordState::SuffixDown, KeyPhase::Release)
                if keycode == TOSHY_LIFECYCLE_WRAPPER_KEYCODE =>
            {
                self.state = ChordState::Idle;
                KeyIntent::LifecycleSuffix
            }
            (ChordState::Idle, KeyPhase::Press) if is_lifecycle => {
                self.state = ChordState::LifecycleDown(keycode);
                lifecycle_intent()
            }
            (ChordState::LifecycleDown(_) | ChordState::AwaitingSuffix, KeyPhase::Press)
                if is_lifecycle =>
            {
                self.state = ChordState::LifecycleDown(keycode);
                lifecycle_intent()
            }
            (_, KeyPhase::Press) => {
                self.state = if keycode == TOSHY_LIFECYCLE_WRAPPER_KEYCODE {
                    ChordState::PrecursorDown
                } else {
                    ChordState::Idle
                };
                KeyIntent::User
            }
            (_, KeyPhase::Release) => KeyIntent::Release,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        device_role, DeviceRole, KeyIntent, KeyPhase, LifecycleChordTracker,
        TOSHY_LIFECYCLE_WRAPPER_KEYCODE,
    };
    use x11rb::protocol::xinput::DeviceType;

    #[test]
    fn discovers_toshy_keyboard_by_role_and_name_not_id() {
        assert_eq!(
            device_role(
                b"XWayKeyz (virtual) Keyboard",
                DeviceType::SLAVE_KEYBOARD,
                true,
            ),
            DeviceRole::ToshyKeyboard
        );
        assert_eq!(
            device_role(
                b"XWayKeyz (virtual) Keyboard",
                DeviceType::SLAVE_POINTER,
                true,
            ),
            DeviceRole::Ignore
        );
    }

    #[test]
    fn accepts_real_pointers_but_rejects_synthetic_sources() {
        assert_eq!(
            device_role(b"bcm5974", DeviceType::SLAVE_POINTER, true),
            DeviceRole::UserPointer
        );
        assert_eq!(
            device_role(
                b"Virtual core XTEST pointer",
                DeviceType::SLAVE_POINTER,
                true,
            ),
            DeviceRole::Ignore
        );
        assert_eq!(
            device_role(
                b"XWayKeyz (virtual) Keyboard",
                DeviceType::SLAVE_POINTER,
                true,
            ),
            DeviceRole::Ignore
        );
    }

    fn classify(sequence: &[(u32, KeyPhase)]) -> Vec<KeyIntent> {
        let mut tracker = LifecycleChordTracker::default();
        sequence
            .iter()
            .map(|(keycode, phase)| tracker.observe(*keycode, *phase, 191, 192))
            .collect()
    }

    #[test]
    fn physical_close_sequence_never_confirms_user_intent() {
        let sequence = [
            (TOSHY_LIFECYCLE_WRAPPER_KEYCODE, KeyPhase::Press),
            (TOSHY_LIFECYCLE_WRAPPER_KEYCODE, KeyPhase::Release),
            (191, KeyPhase::Press),
            (191, KeyPhase::Release),
            (TOSHY_LIFECYCLE_WRAPPER_KEYCODE, KeyPhase::Press),
            (TOSHY_LIFECYCLE_WRAPPER_KEYCODE, KeyPhase::Release),
        ];
        assert!(classify(&sequence)
            .into_iter()
            .all(|intent| !intent.confirms_user_intent()));
    }

    #[test]
    fn physical_quit_sequence_never_confirms_user_intent() {
        let sequence = [
            (105, KeyPhase::Press),
            (105, KeyPhase::Release),
            (192, KeyPhase::Press),
            (192, KeyPhase::Release),
            (105, KeyPhase::Press),
            (105, KeyPhase::Release),
        ];
        assert!(classify(&sequence)
            .into_iter()
            .all(|intent| !intent.confirms_user_intent()));
    }

    #[test]
    fn ordinary_key_and_disambiguated_wrapper_confirm_user_intent() {
        assert_eq!(
            classify(&[(38, KeyPhase::Press)]),
            vec![KeyIntent::User]
        );
        assert_eq!(
            classify(&[
                (105, KeyPhase::Press),
                (105, KeyPhase::Release),
                (38, KeyPhase::Press),
            ]),
            vec![
                KeyIntent::LifecyclePrecursor,
                KeyIntent::LifecyclePrecursor,
                KeyIntent::User,
            ]
        );
    }

    #[test]
    fn grabbed_lifecycle_keys_do_not_require_a_raw_release() {
        let sequence = [
            (105, KeyPhase::Press),
            (105, KeyPhase::Release),
            (191, KeyPhase::Press),
            (105, KeyPhase::Press),
            (105, KeyPhase::Release),
            (192, KeyPhase::Press),
            (105, KeyPhase::Press),
            (105, KeyPhase::Release),
        ];
        assert!(classify(&sequence)
            .into_iter()
            .all(|intent| !intent.confirms_user_intent()));
    }
}

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
                mask: vec![XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_BUTTON_PRESS],
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

#[cfg(test)]
mod tests {
    use super::{device_role, DeviceRole};
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
}

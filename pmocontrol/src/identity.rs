use crate::DeviceId;
use crate::model::DeviceBasicInfo;

pub trait DeviceIdentity {
    fn id(&self) -> DeviceId;
    fn udn(&self) -> &str;
    fn friendly_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn manufacturer(&self) -> &str;
    fn location(&self) -> &str;
    fn server_header(&self) -> &str;

    fn is_a_media_server(&self) -> bool {
        false
    }
    fn is_a_music_renderer(&self) -> bool {
        false
    }

    /// Returns basic device information suitable for event notifications
    fn basic_info(&self) -> DeviceBasicInfo {
        DeviceBasicInfo {
            id: self.id(),
            friendly_name: self.friendly_name().to_string(),
            model_name: self.model_name().to_string(),
            manufacturer: self.manufacturer().to_string(),
        }
    }
}

use crate::DeviceId;

pub trait DeviceIdentity {
    fn id(&self) -> DeviceId ;
    fn udn(&self) -> &str;
    fn friendly_name(&self) -> &str ;
    fn model_name(&self) -> &str ;
    fn manufacturer(&self) -> &str ;
    fn location(&self) -> &str ;
    fn server_header(&self) -> &str;

    fn is_a_media_server(&self) -> bool { false }
    fn is_a_music_renderer(&self)  -> bool { false }
}
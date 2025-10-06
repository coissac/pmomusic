use std::sync::Arc;

use pmoserver::Server;

use crate::devices::errors::DeviceError;
use crate::devices::{Device, DeviceInstance};
use crate::UpnpModel;

pub trait UpnpServer {
 async fn register_device(&mut self, device: Arc<Device>) -> Result<Arc<DeviceInstance>,DeviceError> ;

}

impl UpnpServer for Server {
    async fn register_device(&mut self, device: Arc<Device>) -> Result<Arc<DeviceInstance>,DeviceError> {
        let di = device.create_instance();

        di.register_urls(self).await?;

        Ok(di)
    } 
}
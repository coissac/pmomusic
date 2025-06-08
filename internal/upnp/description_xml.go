package upnp

import (
	"fmt"
)

func generateDeviceDescription(device *DeviceDescription) string {
	return fmt.Sprintf(`<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion>
    <major>1</major>
    <minor>0</minor>
  </specVersion>
  <URLBase>http://%s:%d/</URLBase>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>pmomusic Fake Renderer</friendlyName>
    <manufacturer>GoDLNA</manufacturer>
    <manufacturerURL>http://example.com</manufacturerURL>
    <modelDescription>Fake DLNA Renderer</modelDescription>
    <modelName>pmomusic</modelName>
    <modelNumber>1.0</modelNumber>
    <modelURL>http://example.com/model</modelURL>
    <UDN>%s</UDN>
    <presentationURL>http://%s:%d</presentationURL>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:AVTransport</serviceId>
        <controlURL>/upnp/control/AVTransport</controlURL>
        <eventSubURL>/upnp/event/AVTransport</eventSubURL>
        <SCPDURL>/scpd/AVTransport.xml</SCPDURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:RenderingControl</serviceId>
        <controlURL>/upnp/control/RenderingControl</controlURL>
        <eventSubURL>/upnp/event/RenderingControl</eventSubURL>
        <SCPDURL>/scpd/RenderingControl.xml</SCPDURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:ConnectionManager</serviceId>
        <controlURL>/upnp/control/ConnectionManager</controlURL>
        <eventSubURL>/upnp/event/ConnectionManager</eventSubURL>
        <SCPDURL>/scpd/ConnectionManager.xml</SCPDURL>
      </service>
      <service>
        <serviceType>urn:av-openhome-org:service:Product:1</serviceType>
        <serviceId>urn:av-openhome-org:serviceId:Product</serviceId>
        <controlURL>/upnp/control/Product</controlURL>
        <eventSubURL>/upnp/event/Product</eventSubURL>
        <SCPDURL>/scpd/Product.xml</SCPDURL>
      </service>
      <service>
    		<serviceType>urn:av-openhome-org:service:Playlist:1</serviceType>
    		<serviceId>urn:av-openhome-org:serviceId:Playlist</serviceId>
    		<controlURL>/upnp/control/Playlist</controlURL>
    		<eventSubURL>/upnp/event/Playlist</eventSubURL>
    		<SCPDURL>/scpd/Playlist.xml</SCPDURL>
      </service>
      <service>
    		<serviceType>urn:av-openhome-org:service:Info:1</serviceType>
    		<serviceId>urn:av-openhome-org:serviceId:Info</serviceId>
    		<controlURL>/upnp/control/Info</controlURL>
    		<eventSubURL>/upnp/event/Info</eventSubURL>
    		<SCPDURL>/scpd/Info.xml</SCPDURL>
      </service>
    </serviceList>
  </device>
</root>`, device.IP, device.Port, device.USN, device.IP, device.Port)
}

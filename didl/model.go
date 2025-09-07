package didl

import "encoding/xml"

// DIDLLite représente la racine <DIDL-Lite>
type DIDLLite struct {
	XMLName    xml.Name    `xml:"DIDL-Lite"`
	Xmlns      string      `xml:"xmlns,attr"`
	XmlnsUpnp  string      `xml:"xmlns:upnp,attr,omitempty"`
	XmlnsDc    string      `xml:"xmlns:dc,attr,omitempty"`
	XmlnsDlna  string      `xml:"xmlns:dlna,attr,omitempty"`
	XmlnsSec   string      `xml:"xmlns:sec,attr,omitempty"`
	XmlnsPv    string      `xml:"xmlns:pv,attr,omitempty"`
	Containers []Container `xml:"container"`
	Items      []Item      `xml:"item"`
}

// Container peut contenir d'autres containers ou des items audio
type Container struct {
	ID         string      `xml:"id,attr"`
	ParentID   string      `xml:"parentID,attr"`
	Restricted string      `xml:"restricted,attr,omitempty"`
	ChildCount string      `xml:"childCount,attr,omitempty"`
	Title      string      `xml:"http://purl.org/dc/elements/1.1/ title"`
	Class      string      `xml:"urn:schemas-upnp-org:metadata-1-0/upnp/ class"`
	Containers []Container `xml:"container"`
	Items      []Item      `xml:"item"`
}

// Item représente un objet audio
type Item struct {
	ID         string `xml:"id,attr"`
	ParentID   string `xml:"parentID,attr"`
	Restricted string `xml:"restricted,attr,omitempty"`

	Title               string `xml:"http://purl.org/dc/elements/1.1/ title"`
	Creator             string `xml:"http://purl.org/dc/elements/1.1/ creator,omitempty"`
	Class               string `xml:"urn:schemas-upnp-org:metadata-1-0/upnp/ class"`
	Artist              string `xml:"urn:schemas-upnp-org:metadata-1-0/upnp/ artist,omitempty"`
	Album               string `xml:"urn:schemas-upnp-org:metadata-1-0/upnp/ album,omitempty"`
	Genre               string `xml:"urn:schemas-upnp-org:metadata-1-0/upnp/ genre,omitempty"`
	AlbumArt            string `xml:"urn:schemas-upnp-org:metadata-1-0/upnp/ albumArtURI,omitempty"`
	Date                string `xml:"http://purl.org/dc/elements/1.1/ date,omitempty"`
	OriginalTrackNumber string `xml:"urn:schemas-upnp-org:metadata-1-0/upnp/ originalTrackNumber,omitempty"`

	Ress  []Res  `xml:"res"`
	Descs []Desc `xml:"desc"`
}

// Res correspond aux fichiers média
type Res struct {
	ProtocolInfo    string `xml:"protocolInfo,attr"`
	BitsPerSample   string `xml:"bitsPerSample,attr,omitempty"`
	SampleFrequency string `xml:"sampleFrequency,attr,omitempty"`
	NrAudioChannels string `xml:"nrAudioChannels,attr,omitempty"`
	Duration        string `xml:"duration,attr,omitempty"`
	URL             string `xml:",chardata"`
}

// Desc correspond aux métadonnées optionnelles comme replaygain
type Desc struct {
	ID        string `xml:"id,attr,omitempty"`
	NameSpace string `xml:"nameSpace,attr,omitempty"`
	TrackGain string `xml:"track_gain,omitempty"`
	TrackPeak string `xml:"track_peak,omitempty"`
}

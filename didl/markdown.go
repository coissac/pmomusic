package didl

import (
	"fmt"
	"strings"
)

func (d *DIDLLite) ToMarkdown() string {
	var buf strings.Builder
	buf.WriteString("# DIDL-Lite Document\n\n")

	if len(d.Containers) > 0 {
		buf.WriteString("## Containers\n\n")
		for _, c := range d.Containers {
			c.markdown(&buf, 0)
		}
	}

	if len(d.Items) > 0 {
		buf.WriteString("## Items\n\n")
		for _, i := range d.Items {
			i.markdown(&buf, 0)
		}
	}

	return buf.String()
}

func (c *Container) markdown(buf *strings.Builder, depth int) {
	indent := strings.Repeat("  ", depth)

	buf.WriteString(fmt.Sprintf("%s- **Container**: %s\n", indent, c.Title))
	buf.WriteString(fmt.Sprintf("%s  - ID: `%s`\n", indent, c.ID))
	buf.WriteString(fmt.Sprintf("%s  - ParentID: `%s`\n", indent, c.ParentID))
	buf.WriteString(fmt.Sprintf("%s  - Class: `%s`\n", indent, c.Class))

	if c.Restricted != "" {
		buf.WriteString(fmt.Sprintf("%s  - Restricted: `%s`\n", indent, c.Restricted))
	}
	if c.ChildCount != "" {
		buf.WriteString(fmt.Sprintf("%s  - ChildCount: `%s`\n", indent, c.ChildCount))
	}

	// Sous-conteneurs
	if len(c.Containers) > 0 {
		buf.WriteString(fmt.Sprintf("%s  - Subcontainers:\n", indent))
		for _, sub := range c.Containers {
			sub.markdown(buf, depth+2)
		}
	}

	// Items du conteneur
	if len(c.Items) > 0 {
		buf.WriteString(fmt.Sprintf("%s  - Items:\n", indent))
		for _, item := range c.Items {
			item.markdown(buf, depth+2)
		}
	}

	buf.WriteString("\n")
}

func (i *Item) markdown(buf *strings.Builder, depth int) {
	indent := strings.Repeat("  ", depth)

	buf.WriteString(fmt.Sprintf("%s- **Item**: %s\n", indent, i.Title))
	buf.WriteString(fmt.Sprintf("%s  - ID: `%s`\n", indent, i.ID))
	buf.WriteString(fmt.Sprintf("%s  - ParentID: `%s`\n", indent, i.ParentID))
	buf.WriteString(fmt.Sprintf("%s  - Class: `%s`\n", indent, i.Class))

	if i.Creator != "" {
		buf.WriteString(fmt.Sprintf("%s  - Creator: %s\n", indent, i.Creator))
	}
	if i.Artist != "" {
		buf.WriteString(fmt.Sprintf("%s  - Artist: %s\n", indent, i.Artist))
	}
	if i.Album != "" {
		buf.WriteString(fmt.Sprintf("%s  - Album: %s\n", indent, i.Album))
	}
	if i.Genre != "" {
		buf.WriteString(fmt.Sprintf("%s  - Genre: %s\n", indent, i.Genre))
	}
	if i.AlbumArt != "" {
		buf.WriteString(fmt.Sprintf("%s  - Album Art: ![Cover](%s)\n", indent, i.AlbumArt))
	}
	if i.Date != "" {
		buf.WriteString(fmt.Sprintf("%s  - Date: %s\n", indent, i.Date))
	}
	if i.OriginalTrackNumber != "" {
		buf.WriteString(fmt.Sprintf("%s  - Track: %s\n", indent, i.OriginalTrackNumber))
	}

	// Ressources
	if len(i.Ress) > 0 {
		buf.WriteString(fmt.Sprintf("%s  - Resources:\n", indent))
		for _, res := range i.Ress {
			buf.WriteString(fmt.Sprintf("%s    - URL: %s\n", indent, res.URL))
			buf.WriteString(fmt.Sprintf("%s      - Protocol: `%s`\n", indent, res.ProtocolInfo))
			if res.Duration != "" {
				buf.WriteString(fmt.Sprintf("%s      - Duration: `%s`\n", indent, res.Duration))
			}
			if res.BitsPerSample != "" {
				buf.WriteString(fmt.Sprintf("%s      - BitsPerSample: `%s`\n", indent, res.BitsPerSample))
			}
			if res.SampleFrequency != "" {
				buf.WriteString(fmt.Sprintf("%s      - SampleFrequency: `%s`\n", indent, res.SampleFrequency))
			}
			if res.NrAudioChannels != "" {
				buf.WriteString(fmt.Sprintf("%s      - Channels: `%s`\n", indent, res.NrAudioChannels))
			}
		}
	}

	// Descriptions
	if len(i.Descs) > 0 {
		buf.WriteString(fmt.Sprintf("%s  - Descriptions:\n", indent))
		for _, desc := range i.Descs {
			buf.WriteString(fmt.Sprintf("%s    - Namespace: `%s`\n", indent, desc.NameSpace))
			if desc.TrackGain != "" {
				buf.WriteString(fmt.Sprintf("%s      - Track Gain: `%s`\n", indent, desc.TrackGain))
			}
			if desc.TrackPeak != "" {
				buf.WriteString(fmt.Sprintf("%s      - Track Peak: `%s`\n", indent, desc.TrackPeak))
			}
		}
	}

	buf.WriteString("\n")
}

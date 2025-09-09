package pmostream

import (
	"fmt"
	"os"
	"strings"

	"github.com/gopxl/beep"
	"github.com/gopxl/beep/flac"
	"github.com/gopxl/beep/mp3"
	"github.com/gopxl/beep/vorbis"
	"github.com/gopxl/beep/wav"
)

type streamerWithCloser struct {
	beep.Streamer
	closer func() error
}

func (s *streamerWithCloser) Close() error {
	if s.closer != nil {
		return s.closer()
	}
	return nil
}

func LoadAudio(uri string) (beep.Streamer, beep.Format, error) {
	f, err := os.Open(uri)
	if err != nil {
		return nil, beep.Format{}, err
	}

	var streamer beep.Streamer
	var format beep.Format

	lowerURI := strings.ToLower(uri)
	switch {
	case strings.HasSuffix(lowerURI, ".flac"):
		streamer, format, err = flac.Decode(f)
	case strings.HasSuffix(lowerURI, ".wav"):
		streamer, format, err = wav.Decode(f)
	case strings.HasSuffix(lowerURI, ".mp3"):
		streamer, format, err = mp3.Decode(f)
	case strings.HasSuffix(lowerURI, ".ogg"):
		streamer, format, err = vorbis.Decode(f)
	default:
		f.Close()
		return nil, beep.Format{}, fmt.Errorf("unsupported format: %s", uri)
	}

	if err != nil {
		f.Close()
		return nil, beep.Format{}, err
	}

	return &streamerWithCloser{
		Streamer: streamer,
		closer:   f.Close,
	}, format, nil
}

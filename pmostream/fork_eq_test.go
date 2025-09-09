//go:build cgo
// +build cgo

package pmostream

import (
	"math"
	"testing"
	"time"

	"github.com/gopxl/beep"
)

// TestForkedEQ vérifie que chaque fork peut avoir son propre ParametricEQ appliqué
func TestAudioProcessorWithMasterBuffer(t *testing.T) {
	stream := beep.StreamerFunc(func(samples [][2]float64) (n int, ok bool) {
		for i := range samples {
			v := 0.2 * math.Sin(2*math.Pi*440*float64(i)/44100.0)
			samples[i][0] = v
			samples[i][1] = v
		}
		return len(samples), true
	})

	format := beep.Format{SampleRate: 44100, NumChannels: 2, Precision: 2}
	config := DefaultHiFiConfig()
	master := NewMasterBuffer()

	proc, err := NewAudioProcessor(stream, format, config, master)
	if err != nil {
		t.Fatal(err)
	}

	go proc.Process()
	defer proc.Close()

	time.Sleep(50 * time.Millisecond)

	if proc.buffer.Available() == 0 {
		t.Fatal("buffer should contain data")
	}

	chunk := proc.buffer.Read()
	if chunk == nil || len(chunk) == 0 {
		t.Fatal("failed to read chunk from buffer")
	}
}

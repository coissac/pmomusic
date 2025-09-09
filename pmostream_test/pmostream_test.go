package pmostream_test

import (
	"testing"
	"time"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmostream"
	"github.com/gopxl/beep"
)

// ----------------------- Audio conversions ------------------------
func TestConvertFloat64ToFloat32(t *testing.T) {
	in := [][2]float64{{-2, 2}, {0.5, -0.5}}
	out := pmostream.ConvertFloat64ToFloat32(in)
	if len(out) != 4 {
		t.Fatal("unexpected length")
	}
	if out[0] != -1 || out[1] != 1 {
		t.Fatal("clamp failed")
	}
}

func TestFloat32ToPCM(t *testing.T) {
	in := []float32{0.5, -0.5}
	buf := pmostream.Float32ToPCM(in)
	if len(buf) != 4 {
		t.Fatal("length wrong")
	}
}

func TestFloat32ToBytesAndBack(t *testing.T) {
	in := []float32{0.1, -0.2, 0.3}
	b := pmostream.Float32ToBytes(in)
	out := pmostream.BytesToFloat32(b)
	if len(out) != len(in) {
		t.Fatal("length mismatch")
	}
}

// ----------------------- AudioBuffer ------------------------
func TestAudioBufferWriteRead(t *testing.T) {
	buf := pmostream.NewAudioBuffer(2, pmostream.PCM16)
	data := []byte{1, 2, 3}
	buf.Write(data)
	if buf.Available() != 1 {
		t.Fatal("available wrong")
	}
	r := buf.Read()
	if string(r) != string(data) {
		t.Fatal("read content mismatch")
	}
}

func TestAudioBufferOverwrite(t *testing.T) {
	buf := pmostream.NewAudioBuffer(2, pmostream.PCM16)
	buf.Write([]byte{1})
	buf.Write([]byte{2})
	buf.Write([]byte{3}) // overwrite
	if buf.Available() != 2 {
		t.Fatal("overwrite failed")
	}
}

// ----------------------- MasterBuffer & ForkedBuffer ------------------------
func TestMasterBufferFork(t *testing.T) {
	m := pmostream.NewMasterBuffer()
	m.Write([]byte{1, 2})
	f := m.Fork()
	chunks := f.ReadAll()
	if len(chunks) != 1 {
		t.Fatal("fork readall failed")
	}
}

// ----------------------- ParametricEQ ------------------------
func TestParametricEQ(t *testing.T) {
	stream := beep.StreamerFunc(func(samples [][2]float64) (int, bool) {
		for i := range samples {
			samples[i][0] = 1
			samples[i][1] = 1
		}
		return len(samples), true
	})
	eq := pmostream.NewParametricEQ(stream, pmostream.EQParams{FreqHz: 440, GainDB: 3, Q: 1}, 44100)
	buf := make([][2]float64, 10)
	n, ok := eq.Stream(buf)
	if !ok || n != 10 {
		t.Fatal("stream failed")
	}
}

// ----------------------- AudioProcessor ------------------------
func TestAudioProcessorInit(t *testing.T) {
	stream := beep.StreamerFunc(func(samples [][2]float64) (int, bool) { return len(samples), true })
	format := beep.Format{SampleRate: 44100, NumChannels: 2, Precision: 2}
	config := pmostream.DefaultHiFiConfig()
	proc, err := pmostream.NewAudioProcessor(stream, format, config, nil)
	if err != nil {
		t.Fatal(err)
	}
	proc.SetVolume(1.0)
	proc.Stop()
	proc.Close()
}

// ----------------------- StreamManager ------------------------
func TestStreamManagerAddRemove(t *testing.T) {
	m := pmostream.NewStreamManager()
	stream := beep.StreamerFunc(func(samples [][2]float64) (int, bool) { return len(samples), true })
	format := beep.Format{SampleRate: 44100, NumChannels: 2, Precision: 2}
	config := pmostream.DefaultHiFiConfig()
	proc, _ := pmostream.NewAudioProcessor(stream, format, config, nil)
	m.AddProcessor("p1", proc)
	m.RemoveProcessor("p1")
}

// ----------------------- LoadAudio ------------------------
func TestLoadAudioUnsupported(t *testing.T) {
	_, _, err := pmostream.LoadAudio("file.unsupported")
	if err == nil {
		t.Fatal("expected error")
	}
}

// ----------------------- WaitForData with timeout ------------------------
func TestAudioBufferWaitForDataTimeout(t *testing.T) {
	buf := pmostream.NewAudioBuffer(2, pmostream.PCM16)
	if buf.WaitForData(1, 10*time.Millisecond) {
		t.Fatal("should timeout")
	}
}

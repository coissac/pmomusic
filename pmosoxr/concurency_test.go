//go:build cgo
// +build cgo

package pmosoxr

import (
	"math"
	"sync"
	"testing"
	"time"
)

// genStereoSine génère nFrames frames interlacées float32 (L,R identiques) à amplitude 0.2.
func genStereoSine(nFrames int, freq float64, sr float64) []float32 {
	out := make([]float32, nFrames*2)
	for i := 0; i < nFrames; i++ {
		v := float32(0.2 * math.Sin(2*math.Pi*freq*float64(i)/sr))
		out[2*i] = v
		out[2*i+1] = v
	}
	return out
}

func TestResamplerConcurrentProcess(t *testing.T) {
	inRate := 44100.0
	outRate := 48000.0
	channels := 2
	quality := MQ

	r, err := New(inRate, outRate, channels, quality)
	if err != nil {
		t.Fatalf("failed to create resampler: %v", err)
	}
	defer r.Delete()

	workers := 8
	iterations := 200
	framesPerIter := 256
	var wg sync.WaitGroup
	errCh := make(chan error, workers)
	totalProduced := int64(0)
	var prodMu sync.Mutex

	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			for i := 0; i < iterations; i++ {
				in := genStereoSine(framesPerIter, 440.0+float64(id), inRate)
				ratio := outRate / inRate
				outFrames := int(float64(framesPerIter)*ratio) + 64
				out := make([]float32, outFrames*2)
				consumed, produced, perr := r.Process(in, out)
				if perr != nil {
					errCh <- perr
					return
				}
				if consumed < 0 || produced < 0 {
					errCh <- &testError{"negative sample count"}
					return
				}
				if produced > len(out) {
					errCh <- &testError{"produced > out buffer"}
					return
				}
				prodMu.Lock()
				totalProduced += int64(produced)
				prodMu.Unlock()
				time.Sleep(1 * time.Millisecond)
			}
		}(w)
	}

	wg.Wait()
	close(errCh)

	for e := range errCh {
		t.Fatalf("resampler error: %v", e)
	}

	if totalProduced == 0 {
		t.Fatal("no samples produced")
	}
}

type testError struct {
	s string
}

func (e *testError) Error() string { return e.s }

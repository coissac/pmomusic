//go:build cgo
// +build cgo

package pmosoxr_test

import (
	"sync"
	"testing"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmosoxr"
)

// Test création et suppression d'un Resampler
func TestResamplerLifecycle(t *testing.T) {
	r, err := pmosoxr.New(44100, 48000, 2, pmosoxr.MQ)
	if err != nil {
		t.Fatalf("failed to create resampler: %v", err)
	}
	if r == nil {
		t.Fatal("resampler is nil")
	}

	in := make([]float32, 512)
	out := make([]float32, 512)
	consumed, produced, err := r.Process(in, out)
	if err != nil {
		t.Fatal(err)
	}
	if consumed < 0 || produced < 0 {
		t.Fatal("negative samples")
	}

	// Delete multiple fois ne doit pas panique
	r.Delete()
	r.Delete()
}

// Test Flush sur un Resampler
func TestResamplerFlush(t *testing.T) {
	r, _ := pmosoxr.New(44100, 48000, 2, pmosoxr.MQ)
	defer r.Delete()

	buf := make([]float32, 256)
	n, err := r.Flush(buf)
	if err != nil {
		t.Fatal(err)
	}
	if n < 0 {
		t.Fatal("flush returned negative samples")
	}
}

// Concurrence simple
func TestResamplerConcurrent(t *testing.T) {
	r, _ := pmosoxr.New(44100, 48000, 2, pmosoxr.MQ)
	defer r.Delete()

	var wg sync.WaitGroup
	for i := 0; i < 4; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			in := make([]float32, 128)
			out := make([]float32, 128)
			r.Process(in, out)
		}()
	}
	wg.Wait()
}

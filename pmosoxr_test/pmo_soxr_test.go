// pmosoxr/pmosoxr_test.go
//go:build cgo
// +build cgo

package pmosoxr_test

import (
	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmosoxr"
	"testing"
)

// TestQualityToC vérifie la conversion des enums Quality vers C
func TestQualityToC_Full(t *testing.T) {
	qualities := []pmosoxr.Quality{pmosoxr.QQ, pmosoxr.LQ, pmosoxr.MQ, pmosoxr.HQ, pmosoxr.VHQ}

	for _, q := range qualities {
		// Crée un resampler stéréo de 48000->44100 Hz
		r, err := pmosoxr.New(48000, 44100, 2, q)
		if err != nil {
			t.Errorf("Failed to create resampler with quality %v: %v", q, err)
			continue
		}

		// Remplir un petit buffer d'entrée avec des samples fictifs
		in := []float32{0.0, 0.0, 1.0, -1.0, 0.5, -0.5, 0.25, -0.25}
		out := make([]float32, len(in)*2) // prévoir assez pour la sortie

		consumed, produced, err := r.Process(in, out)
		if err != nil {
			t.Errorf("Process failed for quality %v: %v", q, err)
		} else {
			t.Logf("Quality %v: consumed %d samples, produced %d samples", q, consumed, produced)
		}

		// Tester Flush
		flushOut := make([]float32, len(out))
		n, err := r.Flush(flushOut)
		if err != nil {
			t.Errorf("Flush failed for quality %v: %v", q, err)
		} else {
			t.Logf("Quality %v: flushed %d samples", q, n)
		}

		r.Delete()

	}
}

// TestResamplerCreateDelete vérifie la création et suppression d'un resampler
func TestResamplerCreateDelete(t *testing.T) {
	r, err := pmosoxr.New(44100, 48000, 2, pmosoxr.MQ)
	if err != nil {
		t.Fatalf("failed to create resampler: %v", err)
	}

	r.Delete()

}

// TestResamplerInvalidChannels vérifie la gestion des canaux non stéréo
func TestResamplerInvalidChannels(t *testing.T) {
	_, err := pmosoxr.New(44100, 48000, 1, pmosoxr.MQ)
	if err == nil {
		t.Fatal("expected error for non-stereo channels")
	}
}

// TestProcessErrors vérifie les erreurs de Process sur buffers invalides
func TestProcessErrors(t *testing.T) {
	r, _ := pmosoxr.New(44100, 48000, 2, pmosoxr.MQ)
	defer r.Delete()

	_, _, err := r.Process([]float32{0, 1, 2}, []float32{0})
	if err == nil {
		t.Fatal("expected error for buffer not divisible by channels")
	}

	r.Delete()
	_, _, err = r.Process([]float32{0, 1}, []float32{0, 1})
	if err == nil {
		t.Fatal("expected error for deleted resampler")
	}
}

// TestFlushEmpty vérifie Flush sur buffer vide
func TestFlushEmpty(t *testing.T) {
	r, _ := pmosoxr.New(44100, 48000, 2, pmosoxr.MQ)
	defer r.Delete()

	n, err := r.Flush(nil)
	if err != nil || n != 0 {
		t.Fatalf("Flush empty: got %d, err %v", n, err)
	}
}

package pmostream

import (
	"math"
	"sync"

	"github.com/gopxl/beep"
)

// ParametricEQ applique un égaliseur paramétrique stéréo (Biquad peaking) sur un Streamer.
type ParametricEQ struct {
	input      beep.Streamer
	sampleRate float64
	mu         sync.Mutex

	// Coefficients Biquad
	a0, a1, a2, b1, b2 float64

	// États pour les deux canaux
	x1, x2, y1, y2 [2]float64
}

// EQParams définit les paramètres d'un filtre peaking
type EQParams struct {
	FreqHz float64 // fréquence centrale en Hz
	GainDB float64 // gain en dB
	Q      float64 // facteur de qualité
}

// NewParametricEQ construit un EQ peaking stéréo en interrogeant le streamer pour la fréquence
func NewParametricEQ(input beep.Streamer, params EQParams, sr beep.SampleRate) *ParametricEQ {
	eq := &ParametricEQ{
		input:      input,
		sampleRate: float64(sr),
	}

	eq.setParams(params)
	return eq
}

// setParams calcule les coefficients du Biquad
func (eq *ParametricEQ) setParams(p EQParams) {
	eq.mu.Lock()
	defer eq.mu.Unlock()

	A := math.Pow(10, p.GainDB/40) // conversion dB -> amplitude
	w0 := 2 * math.Pi * p.FreqHz / eq.sampleRate
	alpha := math.Sin(w0) / (2 * p.Q)

	a0 := 1 + alpha/A
	eq.a0 = 1
	eq.a1 = -2 * math.Cos(w0) / a0
	eq.a2 = (1 - alpha/A) / a0
	eq.b1 = 2 * math.Cos(w0) * -1 / a0
	eq.b2 = (1 - alpha*A) / a0
}

// Stream applique l'EQ sur un chunk stéréo float64 [][2]float64
func (eq *ParametricEQ) Stream(samples [][2]float64) (n int, ok bool) {
	eq.mu.Lock()
	defer eq.mu.Unlock()

	if len(samples) == 0 {
		return 0, true
	}

	for i := range samples {
		for ch := 0; ch < 2; ch++ {
			x := samples[i][ch]
			y := eq.a0*x + eq.a1*eq.x1[ch] + eq.a2*eq.x2[ch] - eq.b1*eq.y1[ch] - eq.b2*eq.y2[ch]

			eq.x2[ch] = eq.x1[ch]
			eq.x1[ch] = x
			eq.y2[ch] = eq.y1[ch]
			eq.y1[ch] = y

			samples[i][ch] = y
		}
	}
	return len(samples), true
}

// Close libère les ressources (pas nécessaire ici mais pour interface uniforme)
func (eq *ParametricEQ) Close() error {
	return nil
}

// Wrap permet d'utiliser ParametricEQ comme beep.Streamer
func (eq *ParametricEQ) Streamer() beep.Streamer {
	return beep.StreamerFunc(func(samples [][2]float64) (n int, ok bool) {
		return eq.Stream(samples)
	})
}

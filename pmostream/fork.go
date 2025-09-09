package pmostream

import (
	"fmt"
	"sync"
	"time"

	"github.com/gopxl/beep"
)

// MasterBuffer est un buffer central qui permet de fork un flux vers plusieurs processors
type MasterBuffer struct {
	mu     sync.RWMutex
	cond   *sync.Cond
	chunks [][]byte
	closed bool
}

// NewMasterBuffer crée un buffer central
func NewMasterBuffer() *MasterBuffer {
	mb := &MasterBuffer{}
	mb.cond = sync.NewCond(&mb.mu)
	return mb
}

// Write ajoute un chunk au buffer central
func (mb *MasterBuffer) Write(chunk []byte) {
	if len(chunk) == 0 {
		return
	}

	mb.mu.Lock()
	defer mb.mu.Unlock()

	if mb.closed {
		return
	}

	data := make([]byte, len(chunk))
	copy(data, chunk)

	mb.chunks = append(mb.chunks, data)
	mb.cond.Broadcast()
}

// Fork crée un lecteur indépendant pour ce buffer
func (mb *MasterBuffer) Fork() *ForkedBuffer {
	mb.mu.RLock()
	defer mb.mu.RUnlock()

	return &ForkedBuffer{
		master: mb,
		index:  len(mb.chunks),
	}
}

// Close ferme le buffer et notifie tous les lecteurs
func (mb *MasterBuffer) Close() {
	mb.mu.Lock()
	defer mb.mu.Unlock()
	mb.closed = true
	mb.cond.Broadcast()
}

// ForkedBuffer permet à un processor forké de lire indépendamment
type ForkedBuffer struct {
	master *MasterBuffer
	index  int
}

func (fb *ForkedBuffer) ReadAll() [][]byte {
	fb.master.mu.Lock() // Lock au lieu de RLock
	defer fb.master.mu.Unlock()

	if fb.index >= len(fb.master.chunks) {
		return nil
	}

	result := make([][]byte, len(fb.master.chunks)-fb.index)
	for i := fb.index; i < len(fb.master.chunks); i++ {
		result[i-fb.index] = fb.master.chunks[i]
	}
	fb.index = len(fb.master.chunks)
	return result
}

func (fb *ForkedBuffer) WaitForData(timeoutMs int) bool {
	deadline := time.Now().Add(time.Duration(timeoutMs) * time.Millisecond)
	fb.master.mu.Lock()
	defer fb.master.mu.Unlock()

	for fb.index >= len(fb.master.chunks) && !fb.master.closed {
		remaining := time.Until(deadline)
		if remaining <= 0 {
			return false
		}
		fb.master.cond.Wait()
	}
	return fb.index < len(fb.master.chunks)
}

// ForkStreamer crée plusieurs streamers à partir d'un streamer source en utilisant un MasterBuffer
func ForkStreamer(streamer beep.Streamer, format beep.Format, config HiFiConfig, nForks int) ([]beep.Streamer, error) {
	if nForks < 1 {
		return nil, fmt.Errorf("nForks must be at least 1")
	}

	// Créer un MasterBuffer
	master := NewMasterBuffer()

	// Créer un processeur principal qui alimente le MasterBuffer
	mainProc, err := NewAudioProcessor(streamer, format, config, master)
	if err != nil {
		return nil, err
	}

	// Démarrer le traitement principal
	go mainProc.Process()

	// Créer des streamers forké
	forks := make([]beep.Streamer, nForks)
	for i := 0; i < nForks; i++ {
		forkedBuffer := master.Fork()
		forks[i] = &forkedStreamer{
			fb:     forkedBuffer,
			config: config,
		}
	}

	return forks, nil
}

// forkedStreamer implémente beep.Streamer pour lire depuis un ForkedBuffer
type forkedStreamer struct {
	fb     *ForkedBuffer
	config HiFiConfig
}

func (fs *forkedStreamer) Stream(samples [][2]float64) (n int, ok bool) {
	if !fs.fb.WaitForData(100) {
		return 0, true
	}

	chunks := fs.fb.ReadAll()
	if len(chunks) == 0 {
		return 0, true
	}

	// Concaténer tous les chunks
	var totalSize int
	for _, chunk := range chunks {
		totalSize += len(chunk)
	}

	combined := make([]byte, 0, totalSize)
	for _, chunk := range chunks {
		combined = append(combined, chunk...)
	}

	// Convertir en float32 selon le format
	var allData []float32
	if fs.config.Format == Float32 {
		allData = BytesToFloat32(combined)
	} else {
		allData = PcmToFloat32(combined)
	}

	if allData == nil {
		return 0, true
	}

	numSamples := len(allData) / 2
	if numSamples > len(samples) {
		numSamples = len(samples)
	}

	for i := 0; i < numSamples; i++ {
		if 2*i+1 < len(allData) {
			samples[i][0] = float64(allData[2*i])
			samples[i][1] = float64(allData[2*i+1])
		}
	}

	return numSamples, true
}

func (fs *forkedStreamer) Err() error {
	return nil
}

// ReadAll retourne tous les chunks disponibles dans le buffer central
func (mb *MasterBuffer) ReadAll() [][]byte {
	mb.mu.RLock()
	defer mb.mu.RUnlock()

	if mb.closed || len(mb.chunks) == 0 {
		return nil
	}

	// Créer une copie de tous les chunks
	result := make([][]byte, len(mb.chunks))
	for i, chunk := range mb.chunks {
		result[i] = make([]byte, len(chunk))
		copy(result[i], chunk)
	}

	return result
}

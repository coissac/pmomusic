package pmostream

import (
	"encoding/binary"
	"log"
	"net/http"
	"sync"
	"time"

	"github.com/gopxl/beep"
)

// StreamManager gère plusieurs AudioProcessor et clients HTTP
type StreamManager struct {
	processors map[string]*AudioProcessor
	mu         sync.RWMutex
}

// NewStreamManager crée un gestionnaire de flux audio
func NewStreamManager() *StreamManager {
	return &StreamManager{
		processors: make(map[string]*AudioProcessor),
	}
}

// AddProcessor ajoute un processeur audio et démarre sa boucle Process
func (m *StreamManager) AddProcessor(id string, processor *AudioProcessor) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if _, exists := m.processors[id]; exists {
		log.Printf("Processor with id %s already exists", id)
		return
	}
	m.processors[id] = processor

	go func() {
		if err := processor.Process(); err != nil {
			log.Printf("Processor error for %s: %v", id, err)
		}
	}()
}

// RemoveProcessor arrête et supprime un processeur audio
func (m *StreamManager) RemoveProcessor(id string) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if processor, exists := m.processors[id]; exists {
		processor.Stop()
		processor.Close()
		delete(m.processors, id)
	}
}

// GetHandler retourne un handler HTTP pour streamer un flux audio
func (m *StreamManager) GetHandler(id string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		m.mu.RLock()
		processor, exists := m.processors[id]
		m.mu.RUnlock()

		if !exists {
			http.Error(w, "Stream not found", http.StatusNotFound)
			return
		}

		buffer := processor.GetBuffer()
		config := processor.config

		w.Header().Set("Content-Type", "audio/wav")
		w.Header().Set("Cache-Control", "no-cache")
		w.Header().Set("Connection", "keep-alive")
		w.Header().Set("Access-Control-Allow-Origin", "*")

		flusher, ok := w.(http.Flusher)
		if !ok {
			http.Error(w, "Streaming not supported", http.StatusInternalServerError)
			return
		}

		// Écrire l'en-tête WAV pour streaming
		if err := writeWavHeader(w, config.TargetSampleRate, config.Format); err != nil {
			log.Printf("Failed to write WAV header: %v", err)
			return
		}
		flusher.Flush()

		clientDone := r.Context().Done()

		for {
			select {
			case <-clientDone:
				log.Printf("Client disconnected: %s", id)
				return
			default:
			}

			// Attendre les données avec timeout
			if !buffer.WaitForData(1, 100*time.Millisecond) {
				continue
			}

			chunk := buffer.Read()
			if chunk == nil {
				continue
			}

			// Envoyer le chunk au client
			if _, err := w.Write(chunk); err != nil {
				log.Printf("Write error: %v", err)
				return
			}
			flusher.Flush()
		}
	}
}

// writeWavHeader écrit un en-tête WAV pour streaming
func writeWavHeader(w http.ResponseWriter, sampleRate beep.SampleRate, format SampleFormat) error {
	var audioFormat uint16 = 1 // PCM
	var bitsPerSample uint16 = 16

	if format == Float32 {
		audioFormat = 3 // IEEE_FLOAT
		bitsPerSample = 32
	}

	numChannels := uint16(2)
	blockAlign := numChannels * bitsPerSample / 8
	byteRate := uint32(sampleRate) * uint32(blockAlign)

	header := make([]byte, 44)
	copy(header[0:4], "RIFF")
	binary.LittleEndian.PutUint32(header[4:8], 0xFFFFFFFF) // Taille inconnue pour streaming
	copy(header[8:12], "WAVE")
	copy(header[12:16], "fmt ")
	binary.LittleEndian.PutUint32(header[16:20], 16)
	binary.LittleEndian.PutUint16(header[20:22], audioFormat)
	binary.LittleEndian.PutUint16(header[22:24], numChannels)
	binary.LittleEndian.PutUint32(header[24:28], uint32(sampleRate))
	binary.LittleEndian.PutUint32(header[28:32], byteRate)
	binary.LittleEndian.PutUint16(header[32:34], blockAlign)
	binary.LittleEndian.PutUint16(header[34:36], bitsPerSample)
	copy(header[36:40], "data")
	binary.LittleEndian.PutUint32(header[40:44], 0xFFFFFFFF) // Taille inconnue pour streaming

	_, err := w.Write(header)
	return err
}

package pmostream

import (
	"fmt"
	"math"
	"sync"
	"sync/atomic"
	"time"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmosoxr"
	"github.com/gopxl/beep"
	"github.com/gopxl/beep/effects"
)

type SampleFormat int

const (
	Float32 SampleFormat = iota
	PCM16
)

type HiFiConfig struct {
	TargetSampleRate beep.SampleRate
	BufferSeconds    int
	ChunkSize        int
	ResampleQuality  pmosoxr.Quality
	Volume           float64
	Format           SampleFormat
}

func DefaultHiFiConfig() HiFiConfig {
	return HiFiConfig{
		TargetSampleRate: 48000,
		BufferSeconds:    5,
		ChunkSize:        1024,
		ResampleQuality:  pmosoxr.HQ,
		Volume:           0.0,
		Format:           PCM16,
	}
}

type AudioProcessor struct {
	config       HiFiConfig
	streamer     beep.Streamer
	format       beep.Format
	resampler    *pmosoxr.Resampler
	volume       *effects.Volume
	volumeValue  float64
	buffer       *AudioBuffer
	masterBuffer *MasterBuffer
	processMutex sync.Mutex
	running      atomic.Bool
	wg           sync.WaitGroup
	resampleBuf  []float32
	volumeMu     sync.Mutex
	streamDone   atomic.Bool
}

func NewAudioProcessor(streamer beep.Streamer, format beep.Format, config HiFiConfig, master *MasterBuffer) (*AudioProcessor, error) {
	if streamer == nil && master == nil {
		return nil, fmt.Errorf("streamer cannot be nil if no master buffer is provided")
	}

	if format.NumChannels != 2 {
		return nil, fmt.Errorf("only stereo format is supported")
	}

	var vol *effects.Volume
	if streamer != nil {
		vol = &effects.Volume{
			Streamer: streamer,
			Base:     2,
			Volume:   config.Volume,
			Silent:   false,
		}
	}

	var resampler *pmosoxr.Resampler
	var err error
	if streamer != nil && format.SampleRate != config.TargetSampleRate {
		resampler, err = pmosoxr.New(float64(format.SampleRate), float64(config.TargetSampleRate), 2, config.ResampleQuality)
		if err != nil {
			return nil, fmt.Errorf("failed to create resampler: %w", err)
		}
	}

	sampleSize := 4
	if config.Format == PCM16 {
		sampleSize = 2
	}
	bytesPerSecond := int(config.TargetSampleRate) * sampleSize * 2 // stéréo
	bufferSize := (bytesPerSecond * config.BufferSeconds) / config.ChunkSize
	if bufferSize < 1 {
		bufferSize = 1
	}

	ap := &AudioProcessor{
		config:       config,
		streamer:     streamer,
		format:       format,
		resampler:    resampler,
		volume:       vol,
		volumeValue:  config.Volume,
		buffer:       NewAudioBuffer(bufferSize, config.Format),
		masterBuffer: master,
	}
	ap.running.Store(true)
	return ap, nil
}

func (p *AudioProcessor) GetBuffer() *AudioBuffer {
	return p.buffer
}

func (p *AudioProcessor) SetVolume(volume float64) {
	p.volumeMu.Lock()
	defer p.volumeMu.Unlock()
	p.volumeValue = volume
	if p.volume != nil {
		p.volume.Volume = volume
	}
}

func (p *AudioProcessor) Stop() {
	p.running.Store(false)
}

func (p *AudioProcessor) Close() error {
	p.Stop()
	p.wg.Wait()
	p.processMutex.Lock()
	defer p.processMutex.Unlock()

	if p.resampler != nil {
		p.resampler.Delete()
	}
	if closer, ok := p.streamer.(interface{ Close() error }); ok {
		return closer.Close()
	}
	return nil
}

// Process lit depuis le streamer ou le master buffer, applique resampling + volume et écrit dans le buffer
func (p *AudioProcessor) Process() error {
	if !p.running.Load() {
		return nil
	}

	p.wg.Add(1)
	defer p.wg.Done()

	chunkSize := p.config.ChunkSize
	if chunkSize <= 0 {
		chunkSize = 1024
	}

	for p.running.Load() {
		var samples [][2]float64

		// 1) Lire depuis le streamer
		if p.streamer != nil {
			samples = make([][2]float64, chunkSize)
			n, ok := p.streamer.Stream(samples)
			if !ok {
				p.streamDone.Store(true)
				break
			}
			samples = samples[:n]
			if n == 0 {
				time.Sleep(5 * time.Millisecond)
				continue
			}
		} else if p.masterBuffer != nil {
			// Lecture via ForkedBuffer
			chunks := p.masterBuffer.ReadAll()
			if len(chunks) == 0 {
				time.Sleep(5 * time.Millisecond)
				continue
			}

			samples = make([][2]float64, 0)
			for _, c := range chunks {
				var fs []float32
				if p.config.Format == Float32 {
					fs = BytesToFloat32(c)
				} else {
					fs = PcmToFloat32(c)
				}
				if len(fs)%2 != 0 {
					continue
				}
				for i := 0; i < len(fs); i += 2 {
					samples = append(samples, [2]float64{float64(fs[i]), float64(fs[i+1])})
				}
			}
			if len(samples) == 0 {
				time.Sleep(5 * time.Millisecond)
				continue
			}
		} else {
			time.Sleep(5 * time.Millisecond)
			continue
		}

		// 2) Resampler si nécessaire
		var processed []float32
		if p.resampler != nil {
			if len(p.resampleBuf) < len(samples)*2 {
				p.resampleBuf = make([]float32, len(samples)*2)
			}
			inBuf := ConvertFloat64ToFloat32(samples)
			_, np, err := p.resampler.Process(inBuf, p.resampleBuf)
			if err != nil {
				return err
			}
			processed = p.resampleBuf[:np]
		} else {
			processed = ConvertFloat64ToFloat32(samples)
		}

		// 3) Appliquer volume
		p.volumeMu.Lock()
		volumeFactor := float32(math.Pow(2, p.volumeValue))
		p.volumeMu.Unlock()
		for i := range processed {
			processed[i] *= volumeFactor
		}

		// 4) Convertir et écrire
		var chunk []byte
		if p.config.Format == Float32 {
			chunk = Float32ToBytes(processed)
		} else {
			chunk = Float32ToPCM(processed)
		}
		p.buffer.Write(chunk)
	}

	// Flush resampler à la fin uniquement
	if p.resampler != nil {
		flushBuf := make([]float32, 4096)
		for {
			_, np, err := p.resampler.Process(nil, flushBuf)
			if err != nil {
				return err
			}
			if np == 0 {
				break
			}

			processed := flushBuf[:np]
			p.volumeMu.Lock()
			volumeFactor := float32(math.Pow(2, p.volumeValue))
			p.volumeMu.Unlock()
			for i := range processed {
				processed[i] *= volumeFactor
			}

			var chunk []byte
			if p.config.Format == Float32 {
				chunk = Float32ToBytes(processed)
			} else {
				chunk = Float32ToPCM(processed)
			}
			p.buffer.Write(chunk)
		}
	}

	p.buffer.Close() // fermer uniquement à la fin
	return nil
}

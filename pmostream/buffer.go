package pmostream

import (
	"sync"
	"time"

	log "github.com/sirupsen/logrus"
)

type AudioBuffer struct {
	mu        sync.Mutex
	cond      *sync.Cond
	chunks    [][]byte
	size      int
	available int
	format    SampleFormat
	readPos   int
	writePos  int
	closed    bool
}

func NewAudioBuffer(size int, format SampleFormat) *AudioBuffer {
	log.Infof("New AudioBuffer with size %d", size)
	ab := &AudioBuffer{
		chunks: make([][]byte, size),
		size:   size,
		format: format,
	}
	ab.cond = sync.NewCond(&ab.mu)
	return ab
}

func (ab *AudioBuffer) Available() int {
	ab.mu.Lock()
	defer ab.mu.Unlock()
	return ab.available
}

// Write ajoute un chunk dans le buffer (écrase le plus ancien si plein)
func (ab *AudioBuffer) Write(chunk []byte) {
	ab.mu.Lock()
	defer ab.mu.Unlock()
	if ab.closed {
		return
	}

	ab.chunks[ab.writePos] = chunk
	ab.writePos = (ab.writePos + 1) % ab.size

	if ab.available < ab.size {
		ab.available++
	} else {
		// tampon plein, on écrase → avancer readPos
		ab.readPos = (ab.readPos + 1) % ab.size
	}

	ab.cond.Broadcast()
}

func (ab *AudioBuffer) Read() []byte {
	ab.mu.Lock()
	defer ab.mu.Unlock()

	if ab.available == 0 {
		return nil
	}

	chunk := ab.chunks[ab.readPos]
	ab.chunks[ab.readPos] = nil
	ab.readPos = (ab.readPos + 1) % ab.size
	ab.available--

	return chunk
}

// WaitForData attend qu'au moins n chunks soient disponibles ou timeout/closed
func (ab *AudioBuffer) WaitForData(n int, timeout time.Duration) bool {
	ab.mu.Lock()
	defer ab.mu.Unlock()

	if n <= 0 {
		n = 1
	}

	if timeout <= 0 {
		for ab.available < n && !ab.closed {
			ab.cond.Wait()
		}
		return ab.available >= n && !ab.closed
	}

	deadline := time.Now().Add(timeout)
	for ab.available < n && !ab.closed {
		remaining := time.Until(deadline)
		if remaining <= 0 {
			return false
		}
		waitCondWithTimeout(ab.cond, remaining)
	}

	return ab.available >= n && !ab.closed
}

// Wait est un raccourci pour WaitForData(1, 0)
func (ab *AudioBuffer) Wait() {
	ab.WaitForData(1, 0)
}

func (ab *AudioBuffer) Close() {
	ab.mu.Lock()
	defer ab.mu.Unlock()
	if !ab.closed {
		ab.closed = true
		ab.cond.Broadcast()
	}
}

// Fonction helper pour attendre une sync.Cond avec timeout
func waitCondWithTimeout(c *sync.Cond, d time.Duration) bool {
	timer := time.NewTimer(d)
	done := make(chan struct{})

	go func() {
		c.L.Lock()
		defer c.L.Unlock()
		c.Wait()
		close(done)
	}()

	select {
	case <-done:
		if !timer.Stop() {
			<-timer.C
		}
		return true
	case <-timer.C:
		return false
	}
}

//go:build cgo
// +build cgo

package pmosoxr

/*
#cgo CFLAGS: -I${SRCDIR}/../C/include
#cgo LDFLAGS: ${SRCDIR}/../C/lib/libsoxr.a -lomp

#include <stdlib.h>
#include <soxr.h>
*/
import "C"

import (
	"errors"
	"sync"
	"unsafe"
)

type Resampler struct {
	handle   C.soxr_t
	channels int
	mu       sync.Mutex
	deleted  bool
}

func New(inRate, outRate float64, channels int, q Quality) (*Resampler, error) {
	if channels != 2 {
		return nil, errors.New("only stereo supported")
	}

	var err C.soxr_error_t
	qspec := q.toC()

	handle := C.soxr_create(
		C.double(inRate),
		C.double(outRate),
		C.uint(channels),
		&err,
		nil,
		&qspec,
		nil,
	)
	if handle == nil {
		if err != nil {
			return nil, errors.New(C.GoString(err))
		}
		return nil, errors.New("soxr_create failed without error message")
	}
	return &Resampler{handle: handle, channels: channels}, nil
}

func (r *Resampler) Process(in []float32, out []float32) (consumedSamples int, producedSamples int, err error) {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.deleted {
		return 0, 0, errors.New("resampler deleted")
	}
	if r.handle == nil {
		return 0, 0, errors.New("resampler not initialized")
	}
	if len(in)%r.channels != 0 || len(out)%r.channels != 0 {
		return 0, 0, errors.New("buffer size not divisible by channel count")
	}

	var idone, odone C.size_t
	var inPtr C.soxr_in_t
	var outPtr C.soxr_out_t

	if len(in) > 0 {
		inPtr = C.soxr_in_t(unsafe.Pointer(&in[0]))
	}
	if len(out) > 0 {
		outPtr = C.soxr_out_t(unsafe.Pointer(&out[0]))
	}

	st := C.soxr_process(
		r.handle,
		inPtr, C.size_t(len(in)/r.channels),
		&idone,
		outPtr, C.size_t(len(out)/r.channels),
		&odone,
	)
	if st != nil {
		return 0, 0, errors.New(C.GoString(st))
	}
	return int(idone) * r.channels, int(odone) * r.channels, nil
}

func (r *Resampler) Flush(out []float32) (int, error) {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.deleted {
		return 0, errors.New("resampler deleted")
	}
	if r.handle == nil {
		return 0, errors.New("resampler not initialized")
	}

	var odone C.size_t
	if len(out) == 0 {
		return 0, nil
	}
	st := C.soxr_process(r.handle, nil, 0, nil,
		C.soxr_out_t(unsafe.Pointer(&out[0])), C.size_t(len(out)/r.channels), &odone)
	if st != nil {
		return 0, errors.New(C.GoString(st))
	}
	return int(odone) * r.channels, nil
}

func (r *Resampler) Delete() {
	r.mu.Lock()
	defer r.mu.Unlock()

	if !r.deleted && r.handle != nil {
		C.soxr_delete(r.handle)
		r.handle = nil
		r.deleted = true
	}
}

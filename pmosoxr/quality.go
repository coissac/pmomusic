package pmosoxr

/*
#cgo CFLAGS: -I${SRCDIR}/../C/include

#include <soxr.h>
#include <stdlib.h>

static soxr_quality_spec_t q_spec(int q) {
    switch(q) {
        case 0: return soxr_quality_spec(SOXR_QQ, 0);
        case 1: return soxr_quality_spec(SOXR_LQ, 0);
        case 2: return soxr_quality_spec(SOXR_MQ, 0);
        case 3: return soxr_quality_spec(SOXR_HQ, 0);
        case 4: return soxr_quality_spec(SOXR_VHQ, 0);
        default: return soxr_quality_spec(SOXR_MQ, 0);
    }
}
*/
import "C"

type Quality int

const (
	QQ Quality = iota
	LQ
	MQ
	HQ
	VHQ
)

func (q Quality) toC() C.soxr_quality_spec_t {
	return C.q_spec(C.int(q))
}

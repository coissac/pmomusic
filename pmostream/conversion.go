package pmostream

import (
	"encoding/binary"
	"math"
)

func ConvertFloat64ToFloat32(samples [][2]float64) []float32 {
	if len(samples) == 0 {
		return nil
	}
	result := make([]float32, len(samples)*2)
	for i, s := range samples {
		result[2*i] = float32(clamp(s[0], -1.0, 1.0))
		result[2*i+1] = float32(clamp(s[1], -1.0, 1.0))
	}
	return result
}

func ConvertFloat64ToPCM(samples [][2]float64) []byte {
	if len(samples) == 0 {
		return nil
	}
	buf := make([]byte, len(samples)*4)
	for i, s := range samples {
		l := int16(clamp(s[0], -1.0, 1.0) * 32767.0)
		r := int16(clamp(s[1], -1.0, 1.0) * 32767.0)
		binary.LittleEndian.PutUint16(buf[4*i:], uint16(uint16(l)))
		binary.LittleEndian.PutUint16(buf[4*i+2:], uint16(uint16(r)))
	}
	return buf
}

func Float32ToPCM(samples []float32) []byte {
	if len(samples) == 0 {
		return nil
	}
	buf := make([]byte, len(samples)*2)
	for i, v := range samples {
		val := int16(clamp(float64(v), -1.0, 1.0) * 32767.0)
		binary.LittleEndian.PutUint16(buf[2*i:], uint16(val))
	}
	return buf
}

func Float32ToBytes(samples []float32) []byte {
	if len(samples) == 0 {
		return nil
	}
	buf := make([]byte, len(samples)*4)
	for i, v := range samples {
		binary.LittleEndian.PutUint32(buf[i*4:], math.Float32bits(v))
	}
	return buf
}

func BytesToFloat32(data []byte) []float32 {
	if len(data)%4 != 0 {
		return nil
	}
	result := make([]float32, len(data)/4)
	for i := range result {
		result[i] = math.Float32frombits(binary.LittleEndian.Uint32(data[i*4:]))
	}
	return result
}

func PcmToFloat32(data []byte) []float32 {
	if len(data)%2 != 0 {
		return nil
	}
	result := make([]float32, len(data)/2)
	for i := 0; i < len(result); i++ {
		val := int16(binary.LittleEndian.Uint16(data[2*i:]))
		result[i] = float32(val) / 32768.0
	}
	return result
}

func clamp(val, min, max float64) float64 {
	if val < min {
		return min
	}
	if val > max {
		return max
	}
	return val
}

package pmocover

import (
	"bytes"
	"image"
	"image/draw"
	"os"
	"path/filepath"
	"strconv"

	"github.com/chai2010/webp"
	xdraw "golang.org/x/image/draw"
)

// encodeWebP convertit une image.Image en WebP avec compression sans perte.
func encodeWebP(img image.Image) ([]byte, error) {
	var buf bytes.Buffer
	if err := webp.Encode(&buf, img, &webp.Options{Lossless: true}); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

// ensureSquare génère une image carrée paddée transparente
func ensureSquare(img image.Image, size int) image.Image {
	dst := image.NewRGBA(image.Rect(0, 0, size, size))
	draw.Draw(dst, dst.Bounds(), &image.Uniform{C: image.Transparent}, image.Point{}, draw.Src)

	srcBounds := img.Bounds()
	srcW, srcH := srcBounds.Dx(), srcBounds.Dy()

	var scale float64
	if srcW > srcH {
		scale = float64(size) / float64(srcW)
	} else {
		scale = float64(size) / float64(srcH)
	}
	newW, newH := int(float64(srcW)*scale), int(float64(srcH)*scale)

	// redimensionnement avec Catmull-Rom
	tmp := image.NewRGBA(image.Rect(0, 0, newW, newH))
	xdraw.CatmullRom.Scale(tmp, tmp.Bounds(), img, srcBounds, xdraw.Over, nil)

	// centrer l'image redimensionnée dans le carré
	offset := image.Pt((size-newW)/2, (size-newH)/2)
	draw.Draw(dst, tmp.Bounds().Add(offset), tmp, image.Point{}, draw.Over)

	return dst
}

// generateVariant génère une version carrée en WebP pour une taille donnée.
// Si le fichier existe déjà, il est renvoyé directement.
func (c *Cache) generateVariant(pk string, size int) ([]byte, error) {
	variantPath := filepath.Join(c.dir, pk+"."+strconv.Itoa(size)+".webp")

	// si le variant existe déjà, le renvoyer
	if data, err := os.ReadFile(variantPath); err == nil {
		return data, nil
	}

	// lire l'image originale
	origPath := filepath.Join(c.dir, pk+".orig.webp")
	data, err := os.ReadFile(origPath)
	if err != nil {
		return nil, err
	}
	img, _, err := image.Decode(bytes.NewReader(data))
	if err != nil {
		return nil, err
	}

	// générer une image carrée
	sq := ensureSquare(img, size)

	// encoder en WebP
	buf, err := encodeWebP(sq)
	if err != nil {
		return nil, err
	}

	// sauvegarder le variant
	if err := os.WriteFile(variantPath, buf, 0o644); err != nil {
		return nil, err
	}

	return buf, nil
}

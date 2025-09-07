package soap

import (
	"bytes"
	"fmt"
	"strings"
)

// Markdownable représente un objet pouvant se transformer en Markdown
type Markdownable interface {
	ToMarkdown() string
}

// ToMarkdown convertit l'action et ses arguments en Markdown lisible
func (ar *ActionRequest) ToMarkdown() string {
	var buf bytes.Buffer

	// Titre de l'action
	buf.WriteString(fmt.Sprintf("➡️ SOAP Action: %s\n\n", ar.Name))

	for key, val := range ar.Args {
		buf.WriteString(fmt.Sprintf("- **%s**: ", key))

		var content string
		if md, ok := val.(Markdownable); ok {
			content = md.ToMarkdown()
		} else {
			content = fmt.Sprintf("%v", val)
		}

		// Si contenu long ou multi-lignes
		if strings.Contains(content, "\n") || len(content) > 60 {
			// Résumé : première ligne ou URL tronquée
			summary := firstLineOrTruncate(content, 60)
			buf.WriteString(fmt.Sprintf("`%s`\n", summary))
			buf.WriteString("<details>\n\n")

			// Conserver Markdown complet, sans indentation
			buf.WriteString(content)
			if !strings.HasSuffix(content, "\n") {
				buf.WriteString("\n")
			}
			buf.WriteString("</details>\n\n")
		} else if isURL(content) {
			buf.WriteString(fmt.Sprintf("[%s](%s)\n", content, content))
		} else {
			buf.WriteString(fmt.Sprintf("`%s`\n", content))
		}
	}

	return buf.String()
}

// Tronque la première ligne ou l'URL si trop longue
func firstLineOrTruncate(s string, max int) string {
	lines := strings.SplitN(s, "\n", 2)
	first := lines[0]
	if len(first) > max {
		return first[:max] + "…"
	}

	first = strings.TrimLeft(first, "# ")
	return first
}

// Vérifie si une string ressemble à une URL
func isURL(s string) bool {
	return strings.HasPrefix(s, "http://") || strings.HasPrefix(s, "https://")
}

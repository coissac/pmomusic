
## Installing documentation system Doc-Gen

https://github.com/fynnfluegge/doc-comments-ai


for f in upnp/*.go ; do
 dcai/bin/aicomment  --ollama-model deepseek-coder:33b-instruct $f
done
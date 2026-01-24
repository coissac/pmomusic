import { ref, watch, onMounted, nextTick, type Ref } from "vue";

export interface CoverImageOptions {
  maxRetries?: number;
  retryDelay?: number;
  forceReload?: boolean;
}

export function useCoverImage(
  imageUrl: Ref<string | null | undefined>,
  options: CoverImageOptions = {},
) {
  const { maxRetries = 3, retryDelay = 1000, forceReload = true } = options;

  const imageLoaded = ref(false);
  const imageError = ref(false);
  const coverImageRef = ref<HTMLImageElement | null>(null);
  const retryCount = ref(0);
  const currentUrl = ref<string | null>(null);
  const cacheBustedUrl = ref<string | null>(null);
  const isLoadingNewImage = ref(false);

  // Function to check if the image is already loaded (cached)
  function checkImageComplete() {
    nextTick(() => {
      if (
        coverImageRef.value?.complete &&
        coverImageRef.value?.naturalWidth > 0
      ) {
        imageLoaded.value = true;
        imageError.value = false;
        retryCount.value = 0;
      }
    });
  }

  // Simple hash function for URL
  function simpleHash(str: string): string {
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
      const char = str.charCodeAt(i);
      hash = (hash << 5) - hash + char;
      hash = hash & hash; // Convert to 32bit integer
    }
    return Math.abs(hash).toString(36);
  }

  // Function to add cache-busting parameter
  function getCacheBustedUrl(url: string, retry: number): string {
    if (!forceReload && retry === 0) {
      return url;
    }
    const separator = url.includes("?") ? "&" : "?";
    // Use URL hash for stable cache-busting, timestamp only for retries
    const cacheBuster =
      retry > 0
        ? `${simpleHash(url)}_r${retry}_${Date.now()}`
        : simpleHash(url);
    return `${url}${separator}_cb=${cacheBuster}`;
  }

  // Retry loading the image
  function retryLoad() {
    if (!currentUrl.value) return;

    if (retryCount.value < maxRetries) {
      retryCount.value++;
      console.log(
        `[useCoverImage] Retrying image load (${retryCount.value}/${maxRetries}): ${currentUrl.value}`,
      );

      setTimeout(() => {
        if (!currentUrl.value) return;

        // Update cache-busted URL with new retry count
        cacheBustedUrl.value = getCacheBustedUrl(
          currentUrl.value,
          retryCount.value,
        );
        console.log(`[useCoverImage] Retry URL: ${cacheBustedUrl.value}`);
      }, retryDelay * retryCount.value); // Exponential backoff
    } else {
      console.error(
        `[useCoverImage] Max retries (${maxRetries}) reached for: ${currentUrl.value}`,
      );
      imageError.value = true;
    }
  }

  // Handle successful image load
  function handleImageLoad() {
    console.log(
      `[useCoverImage] Image loaded successfully: ${currentUrl.value}`,
    );
    imageLoaded.value = true;
    imageError.value = false;
    retryCount.value = 0;
    isLoadingNewImage.value = false;
  }

  // Handle image load error
  function handleImageError(event: Event) {
    const img = event.target as HTMLImageElement;
    console.warn(
      `[useCoverImage] Image load error (attempt ${retryCount.value + 1}/${maxRetries + 1}): ${img.src}`,
    );

    imageLoaded.value = false;

    // Retry if we haven't reached max retries
    if (retryCount.value < maxRetries) {
      retryLoad();
    } else {
      imageError.value = true;
    }
  }

  // Reset image state when URL changes
  watch(
    imageUrl,
    (newUri, oldUri) => {
      console.log(
        `[useCoverImage] URL changed from "${oldUri}" to "${newUri}"`,
      );

      imageError.value = false;
      retryCount.value = 0;

      // Si c'est un changement d'URL (pas l'initialisation)
      if (oldUri && newUri && oldUri !== newUri) {
        console.log(
          `[useCoverImage] Changing image, keeping old one visible during load`,
        );
        isLoadingNewImage.value = true;
        // On garde imageLoaded à true pour garder l'ancienne image visible
      } else if (!newUri) {
        // Pas d'URL, on cache tout
        imageLoaded.value = false;
        isLoadingNewImage.value = false;
      } else if (!oldUri && newUri) {
        // Initialisation, on part de zéro
        imageLoaded.value = false;
        isLoadingNewImage.value = true;
      }

      currentUrl.value = newUri || null;

      if (newUri) {
        // Generate cache-busted URL
        cacheBustedUrl.value = getCacheBustedUrl(newUri, 0);
        console.log(
          `[useCoverImage] New cache-busted URL: ${cacheBustedUrl.value}`,
        );
      } else {
        cacheBustedUrl.value = null;
      }
    },
    { immediate: true },
  );

  // Check on mount
  onMounted(() => {
    currentUrl.value = imageUrl.value || null;
    if (currentUrl.value) {
      cacheBustedUrl.value = getCacheBustedUrl(currentUrl.value, 0);
    }
    checkImageComplete();
  });

  return {
    imageLoaded,
    imageError,
    coverImageRef,
    cacheBustedUrl,
    handleImageLoad,
    handleImageError,
  };
}

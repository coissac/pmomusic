// Déclare les modules CSS pour que TypeScript les accepte
declare module '*.css'

// Déclare les fichiers SVG pour que TypeScript les accepte
declare module '*.svg' {
  const content: string
  export default content
}

// Déclare les fichiers PNG, JPG si tu en utilises
declare module '*.png'
declare module '*.jpg'
declare module '*.jpeg'
declare module '*.gif'

// Déclare les fichiers JSON si nécessaire
declare module '*.json' {
  const value: any
  export default value
}

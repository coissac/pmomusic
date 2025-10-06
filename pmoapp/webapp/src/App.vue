<template>
  <div>
    <nav>
      <router-link to="/">Accueil</router-link> |
      <router-link to="/logs">Logs</router-link>
    </nav>
    <router-view />
  </div>
</template>

<script setup lang="ts">
// rien à importer
</script>

<style scoped>
nav {
  background: #333;
  width: 100vw;
  padding: 0.5rem;
}
a {
  color: #eee;
  margin: 0 0.5rem;
}
a.router-link-active {
  font-weight: bold;
  text-decoration: underline;
}
</style>

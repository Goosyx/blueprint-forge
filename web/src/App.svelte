<!--
  PT: Casca da aplicação: cabeçalho com navegação, sessão e idioma, e a página da rota atual.
      A página de análise fica sempre montada (só escondida), para não recarregar o motor WASM.
  EN: Application shell: header with navigation, session and language, and the current route's page.
      The analysis page stays mounted (only hidden), so the WASM engine is not reloaded.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { language, languages, t, type Language } from './lib/i18n';
  import { route } from './lib/router';
  import { logout, me, refreshSession, sessionChecked } from './lib/session';
  import Analyze from './routes/Analyze.svelte';
  import Login from './routes/Login.svelte';
  import Register from './routes/Register.svelte';
  import Blueprints from './routes/Blueprints.svelte';
  import BlueprintDetail from './routes/BlueprintDetail.svelte';
  import Projects from './routes/Projects.svelte';
  import ProjectDetail from './routes/ProjectDetail.svelte';
  import Settings from './routes/Settings.svelte';

  const languageNames: Record<Language, string> = { 'pt-BR': 'Português', en: 'English' };
  const needsLogin = $derived(
    ['blueprints', 'blueprint', 'projects', 'project', 'settings'].includes($route.page),
  );

  onMount(() => void refreshSession());
</script>

<div class="app">
  <header>
    <div class="brand">
      <h1><a href="#/">{$t('app.title')}</a></h1>
      <p class="subtitle">{$t('app.subtitle')}</p>
    </div>
    <nav class="nav">
      <a href="#/" class:active={$route.page === 'analyze'}>{$t('nav.analyze')}</a>
      {#if $me}
        <a
          href="#/blueprints"
          class:active={$route.page === 'blueprints' || $route.page === 'blueprint'}
          >{$t('nav.blueprints')}</a
        >
        <a href="#/projects" class:active={$route.page === 'projects' || $route.page === 'project'}
          >{$t('nav.projects')}</a
        >
        <a href="#/settings" class:active={$route.page === 'settings'}>{$t('nav.settings')}</a>
      {/if}
    </nav>
    <div class="session">
      {#if $me}
        <span class="muted">{$me.email}</span>
        <button class="secondary" onclick={() => void logout()}>{$t('auth.logout')}</button>
      {:else if $sessionChecked}
        <a href="#/login">{$t('auth.login')}</a>
      {/if}
      <label class="language">
        <span class="visually-hidden">{$t('language.label')}</span>
        <select bind:value={$language} aria-label={$t('language.label')}>
          {#each languages as lang (lang)}
            <option value={lang}>{languageNames[lang]}</option>
          {/each}
        </select>
      </label>
    </div>
  </header>
  <main>
    <div class="page" class:hidden={$route.page !== 'analyze'}>
      <Analyze />
    </div>
    {#if $route.page !== 'analyze'}
      <div class="page">
        {#if needsLogin && $sessionChecked && !$me}
          <Login />
        {:else if $route.page === 'login'}
          <Login />
        {:else if $route.page === 'register'}
          <Register />
        {:else if $route.page === 'blueprints'}
          <Blueprints />
        {:else if $route.page === 'blueprint'}
          <BlueprintDetail id={$route.id} />
        {:else if $route.page === 'projects'}
          <Projects />
        {:else if $route.page === 'project'}
          <ProjectDetail id={$route.id} />
        {:else if $route.page === 'settings'}
          <Settings />
        {/if}
      </div>
    {/if}
  </main>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    min-height: 100vh;
    box-sizing: border-box;
    padding: 12px 16px;
    gap: 12px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0;
    font-size: 20px;
  }
  h1 a {
    color: inherit;
    text-decoration: none;
  }
  .subtitle {
    margin: 2px 0 0;
    color: var(--muted);
    font-size: 13px;
  }
  .nav {
    display: flex;
    gap: 14px;
    flex-wrap: wrap;
  }
  .nav a {
    color: var(--text);
    text-decoration: none;
    padding: 4px 0;
    border-bottom: 2px solid transparent;
  }
  .nav a.active {
    border-bottom-color: var(--accent);
  }
  .session {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 13px;
  }
  main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .page {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
  }
  .page.hidden {
    display: none;
  }
</style>

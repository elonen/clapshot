<script lang="ts">
    import { onMount } from 'svelte';

    export let html: string;

    // Add an external script to the document, if it hasn't already been added.
    const addScript = async (src: string): Promise<void> => {
      return new Promise((resolve, reject) => {
        const existingScripts = Array.from(document.getElementsByTagName('script'));
        if (existingScripts.some(script => script.src === src)) {
          resolve();
          return;
        }
        const script = document.createElement('script');
        script.src = src;
        script.onload = () => resolve();
        script.onerror = () => reject(`Failed to load script ${src}`);
        document.body.appendChild(script);
      });
    };

    const addStyle = (href: string) => {
      const existingStyles = Array.from(document.getElementsByTagName('link'));
      if (!existingStyles.some(style => style.href === href)) {
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = href;
        document.head.appendChild(link);
      }
    };

    onMount(async () =>
    {
      const container = document.getElementById('container');
      if (!container) {
          console.error('RawHtmlItem: container element not found!');
          return;
      }

      container.innerHTML = html;

      const externalScripts: HTMLScriptElement[] = Array.from(container.querySelectorAll('script[src]'));
      const inlineScripts: HTMLScriptElement[] = Array.from(container.querySelectorAll('script:not([src])'));
      const externalStyles: HTMLLinkElement[] = Array.from(container.querySelectorAll('link[rel="stylesheet"]'));

      const scriptPromises = externalScripts.map(script => addScript(script.src));
      await Promise.all(scriptPromises);

      inlineScripts.forEach(script => {
        // Skip inline scripts with a type other than JavaScript (e.g. text/template)
        if (script.type && script.type !== 'text/javascript') { return; }
        new Function(script.innerHTML)();
      });

      externalStyles.forEach(style => addStyle(style.href));
    });
  </script>

  <div id="container"></div>

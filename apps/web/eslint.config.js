import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import tseslint from 'typescript-eslint';
import prettier from 'eslint-config-prettier';
import globals from 'globals';

export default tseslint.config(
	{
		ignores: [
			'node_modules/**',
			'.svelte-kit/**',
			'build/**',
			'.turbo/**',
			'src/lib/vnc/**'
		]
	},
	js.configs.recommended,
	...tseslint.configs.recommended,
	...svelte.configs['flat/recommended'],
	{
		files: ['**/*.svelte'],
		languageOptions: {
			parserOptions: {
				parser: tseslint.parser
			}
		}
	},
	{
		// TypeScript and Svelte resolve identifiers through type-checking
		// (svelte-check is part of the `check` gate), so the JS-level
		// no-undef rule is redundant for those files.
		files: ['**/*.ts', '**/*.tsx', '**/*.svelte'],
		rules: {
			'no-undef': 'off'
		}
	},
	{
		files: ['src/**/*.js'],
		languageOptions: {
			globals: globals.browser
		}
	},
	{
		// Build/tooling files run under Node, not the browser.
		files: ['*.config.js', '*.config.ts', 'vite.config.*', 'svelte.config.*', 'scripts/**'],
		languageOptions: {
			globals: globals.node
		}
	},
	{
		rules: {
			'@typescript-eslint/no-unused-vars': [
				'error',
				{ argsIgnorePattern: '^_', varsIgnorePattern: '^_' }
			],
			// The app is a static SPA (adapter-static, base '') that resolves
			// routes at runtime from window.location.pathname; navigation is
			// intentionally not routed through SvelteKit's resolve().
			'svelte/no-navigation-without-resolve': 'off'
		}
	},
	prettier
);

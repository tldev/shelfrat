/// <reference types="vitest/config" />
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

/** Strip SvelteKit's CSP header when DANGEROUSLY_DISABLE_CSP=true. Vite's dev
 *  server injects inline scripts and HMR websockets that hash-based CSP can't
 *  cover, so this is needed for local development. */
function stripCsp() {
	if (process.env.DANGEROUSLY_DISABLE_CSP !== 'true') return { name: 'strip-csp-noop' };
	return {
		name: 'strip-csp',
		configureServer(server) {
			server.middlewares.use((_req, res, next) => {
				const origWriteHead = res.writeHead;
				res.writeHead = function (statusCode, ...args) {
					res.removeHeader('content-security-policy');
					return origWriteHead.call(this, statusCode, ...args);
				};
				next();
			});
		}
	};
}

export default defineConfig({
	plugins: [sveltekit(), stripCsp()],
	server: {
		proxy: {
			'/api': {
				target: 'http://localhost:3000',
				changeOrigin: true
			}
		}
	},
	test: {
		include: ['src/**/*.{test,spec}.{js,ts}'],
		globals: true,
		environment: 'jsdom',
	}
});

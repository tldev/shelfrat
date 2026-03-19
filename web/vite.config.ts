/// <reference types="vitest/config" />
import { sveltekit } from '@sveltejs/kit/vite';
import type { Plugin, ViteDevServer } from 'vite';
import { defineConfig } from 'vite';
import type { IncomingMessage, ServerResponse } from 'node:http';

/** Strip SvelteKit's CSP header when DANGEROUSLY_DISABLE_CSP=true. Vite's dev
 *  server injects inline scripts and HMR websockets that hash-based CSP can't
 *  cover, so this is needed for local development. */
function stripCsp(): Plugin {
	if (process.env.DANGEROUSLY_DISABLE_CSP !== 'true') return { name: 'strip-csp-noop' };
	return {
		name: 'strip-csp',
		configureServer(server: ViteDevServer) {
			server.middlewares.use((_req: IncomingMessage, res: ServerResponse, next: () => void) => {
				const origWriteHead = res.writeHead.bind(res);
				res.writeHead = function (this: ServerResponse, statusCode: number, ...args: unknown[]) {
					this.removeHeader('content-security-policy');
					return (origWriteHead as Function).call(this, statusCode, ...args);
				} as typeof res.writeHead;
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

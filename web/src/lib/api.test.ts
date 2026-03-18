import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
	checkSetup,
	setup,
	login,
	getMe,
	listBooks,
	getBook,
	listAuthors,
	listTags,
	listFormats,
	searchBooks,
	coverUrl,
	downloadUrl,
	sendToKindle,
	listUsers,
	getUser,
	createInvite,
	updateUser,
	revokeUser,
	getSettings,
	updateSettings,
	getLibraryInfo,
	triggerScan,
	rebuildIndex,
	getJobs,
	triggerJob,
	getJobRuns,
	updateJobCadence,
	getAuditLog,
	registerWithInvite,
	getOidcStatus,
	getOidcAuthorize
} from './api';

// --- localStorage mock ---
const store: Record<string, string> = {};
const localStorageMock = {
	getItem: vi.fn((key: string) => store[key] ?? null),
	setItem: vi.fn((key: string, value: string) => {
		store[key] = value;
	}),
	removeItem: vi.fn((key: string) => {
		delete store[key];
	}),
	clear: vi.fn(() => {
		for (const k of Object.keys(store)) delete store[k];
	})
};
Object.defineProperty(globalThis, 'localStorage', {
	value: localStorageMock,
	writable: true
});

// --- window.location mock ---
const locationMock = { href: '' };
Object.defineProperty(globalThis, 'window', {
	value: { location: locationMock, localStorage: localStorageMock },
	writable: true
});

// --- fetch mock ---
const fetchMock = vi.fn();
globalThis.fetch = fetchMock;

function okResponse(body: unknown) {
	return {
		ok: true,
		status: 200,
		json: () => Promise.resolve(body)
	};
}

function errorResponse(status: number, body?: unknown) {
	return {
		ok: false,
		status,
		statusText: 'Bad Request',
		json: () => (body ? Promise.resolve(body) : Promise.reject(new Error('no json')))
	};
}

function unauthorizedResponse() {
	return {
		ok: false,
		status: 401,
		statusText: 'Unauthorized',
		json: () => Promise.resolve({ error: 'Unauthorized' })
	};
}

beforeEach(() => {
	fetchMock.mockReset();
	localStorageMock.getItem.mockClear();
	localStorageMock.setItem.mockClear();
	localStorageMock.removeItem.mockClear();
	localStorageMock.clear.mockClear();
	for (const k of Object.keys(store)) delete store[k];
	locationMock.href = '';
});

// =============== URL helpers (pure functions) ===============

describe('coverUrl', () => {
	it('returns correct URL for a book id', () => {
		expect(coverUrl(42)).toBe('/api/v1/books/42/cover');
	});

	it('returns correct URL for id 0', () => {
		expect(coverUrl(0)).toBe('/api/v1/books/0/cover');
	});
});

describe('downloadUrl', () => {
	it('returns correct URL for a book id', () => {
		expect(downloadUrl(7)).toBe('/api/v1/books/7/download');
	});
});

// =============== Auth header / token handling ===============

describe('request token handling', () => {
	it('includes Authorization header when token is in localStorage', async () => {
		store['token'] = 'abc123';
		fetchMock.mockResolvedValueOnce(okResponse({ ok: true }));

		await checkSetup();

		expect(fetchMock).toHaveBeenCalledOnce();
		const [, opts] = fetchMock.mock.calls[0];
		expect(opts.headers['Authorization']).toBe('Bearer abc123');
	});

	it('does not include Authorization header when no token', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ setup_complete: true }));

		await checkSetup();

		const [, opts] = fetchMock.mock.calls[0];
		expect(opts.headers['Authorization']).toBeUndefined();
	});
});

// =============== 401 handling ===============

describe('401 handling', () => {
	it('clears localStorage and redirects to /login on 401', async () => {
		store['token'] = 'expired';
		store['user'] = '{"id":1}';
		fetchMock.mockResolvedValueOnce(unauthorizedResponse());

		await expect(checkSetup()).rejects.toThrow('Unauthorized');

		expect(localStorageMock.removeItem).toHaveBeenCalledWith('token');
		expect(localStorageMock.removeItem).toHaveBeenCalledWith('user');
		expect(locationMock.href).toBe('/login');
	});
});

// =============== Error handling ===============

describe('error handling', () => {
	it('throws error with message from JSON body', async () => {
		fetchMock.mockResolvedValueOnce(errorResponse(400, { error: 'Invalid input' }));
		await expect(checkSetup()).rejects.toThrow('Invalid input');
	});

	it('throws error with message field from JSON body', async () => {
		fetchMock.mockResolvedValueOnce(errorResponse(400, { message: 'Something wrong' }));
		await expect(checkSetup()).rejects.toThrow('Something wrong');
	});

	it('falls back to statusText when JSON parse fails', async () => {
		fetchMock.mockResolvedValueOnce(errorResponse(400));
		await expect(checkSetup()).rejects.toThrow('Bad Request');
	});
});

// =============== Auth endpoints ===============

describe('checkSetup', () => {
	it('calls GET /api/v1/setup/status', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ setup_complete: true }));

		const result = await checkSetup();

		expect(fetchMock).toHaveBeenCalledWith('/api/v1/setup/status', expect.objectContaining({
			headers: expect.objectContaining({ 'Content-Type': 'application/json' })
		}));
		expect(result).toEqual({ setup_complete: true });
	});
});

describe('setup', () => {
	it('calls POST /api/v1/setup with correct body', async () => {
		const data = { username: 'admin', email: 'a@b.com', password: 'pass123' };
		fetchMock.mockResolvedValueOnce(okResponse({ message: 'ok', username: 'admin' }));

		const result = await setup(data);

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/setup');
		expect(opts.method).toBe('POST');
		expect(JSON.parse(opts.body)).toEqual(data);
		expect(result).toEqual({ message: 'ok', username: 'admin' });
	});

	it('includes library_path when provided', async () => {
		const data = { username: 'admin', email: 'a@b.com', password: 'pass', library_path: '/books' };
		fetchMock.mockResolvedValueOnce(okResponse({ message: 'ok', username: 'admin' }));

		await setup(data);

		const body = JSON.parse(fetchMock.mock.calls[0][1].body);
		expect(body.library_path).toBe('/books');
	});
});

describe('login', () => {
	it('calls POST /api/v1/auth/login with username and password', async () => {
		const mockUser = { id: 1, username: 'test', display_name: null, email: 't@t.com', role: 'user', kindle_email: null };
		fetchMock.mockResolvedValueOnce(okResponse({ token: 'tok', user: mockUser }));

		const result = await login('test', 'secret');

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/auth/login');
		expect(opts.method).toBe('POST');
		expect(JSON.parse(opts.body)).toEqual({ username: 'test', password: 'secret' });
		expect(result.token).toBe('tok');
		expect(result.user.username).toBe('test');
	});
});

describe('registerWithInvite', () => {
	it('calls POST /api/v1/users/register/:token with data', async () => {
		const data = { username: 'newuser', email: 'new@test.com', password: 'pw' };
		fetchMock.mockResolvedValueOnce(okResponse({ message: 'ok', username: 'newuser' }));

		const result = await registerWithInvite('inv-token-abc', data);

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/users/register/inv-token-abc');
		expect(opts.method).toBe('POST');
		expect(JSON.parse(opts.body)).toEqual(data);
		expect(result).toEqual({ message: 'ok', username: 'newuser' });
	});
});

describe('getMe', () => {
	it('calls GET /api/v1/auth/me', async () => {
		const mockUser = { id: 1, username: 'me', display_name: null, email: 'me@x.com', role: 'admin', kindle_email: null };
		fetchMock.mockResolvedValueOnce(okResponse(mockUser));

		const result = await getMe();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/auth/me');
		expect(result.username).toBe('me');
	});
});

describe('getOidcStatus', () => {
	it('calls GET /api/v1/auth/oidc/status', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ enabled: true }));

		const result = await getOidcStatus();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/auth/oidc/status');
		expect(result.enabled).toBe(true);
	});
});

describe('getOidcAuthorize', () => {
	it('calls GET /api/v1/auth/oidc/authorize', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ url: 'https://sso.example.com/auth' }));

		const result = await getOidcAuthorize();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/auth/oidc/authorize');
		expect(result.url).toBe('https://sso.example.com/auth');
	});
});

// =============== Books endpoints ===============

describe('listBooks', () => {
	it('calls GET /api/v1/books with no params', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ books: [], total: 0, limit: 20, offset: 0 }));

		const result = await listBooks();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/books');
		expect(result.total).toBe(0);
	});

	it('includes query params when provided', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ books: [], total: 0, limit: 10, offset: 5 }));

		await listBooks({ sort: 'title', limit: 10, offset: 5, author: 'Tolkien', tag: 'fantasy', format: 'epub' });

		const url = fetchMock.mock.calls[0][0] as string;
		expect(url).toContain('sort=title');
		expect(url).toContain('limit=10');
		expect(url).toContain('offset=5');
		expect(url).toContain('author=Tolkien');
		expect(url).toContain('tag=fantasy');
		expect(url).toContain('format=epub');
	});

	it('omits undefined params', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ books: [], total: 0, limit: 20, offset: 0 }));

		await listBooks({ sort: 'title' });

		const url = fetchMock.mock.calls[0][0] as string;
		expect(url).toContain('sort=title');
		expect(url).not.toContain('limit');
		expect(url).not.toContain('offset');
		expect(url).not.toContain('author');
	});
});

describe('getBook', () => {
	it('calls GET /api/v1/books/:id', async () => {
		const mockDetail = {
			book: { id: 5, filename: 'test.epub', file_hash: 'h', file_format: 'epub', file_size_bytes: 100, added_at: '', last_seen_at: '', missing: false },
			metadata: null,
			authors: [],
			tags: []
		};
		fetchMock.mockResolvedValueOnce(okResponse(mockDetail));

		const result = await getBook(5);

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/books/5');
		expect(result.book.id).toBe(5);
	});
});

describe('listAuthors', () => {
	it('calls GET /api/v1/authors', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ authors: [{ name: 'Author A', book_count: 3 }] }));

		const result = await listAuthors();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/authors');
		expect(result.authors).toHaveLength(1);
		expect(result.authors[0].name).toBe('Author A');
	});
});

describe('listTags', () => {
	it('calls GET /api/v1/tags', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ tags: [{ name: 'fiction', book_count: 5 }] }));

		const result = await listTags();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/tags');
		expect(result.tags[0].name).toBe('fiction');
	});
});

describe('listFormats', () => {
	it('calls GET /api/v1/formats', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ formats: [{ name: 'epub', book_count: 10 }] }));

		const result = await listFormats();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/formats');
		expect(result.formats[0].name).toBe('epub');
	});
});

describe('searchBooks', () => {
	it('calls GET /api/v1/books/search with q param', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ books: [], query: 'test' }));

		const result = await searchBooks('test');

		const url = fetchMock.mock.calls[0][0] as string;
		expect(url).toContain('/api/v1/books/search');
		expect(url).toContain('q=test');
		expect(result.query).toBe('test');
	});

	it('includes limit param when provided', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ books: [], query: 'foo' }));

		await searchBooks('foo', 5);

		const url = fetchMock.mock.calls[0][0] as string;
		expect(url).toContain('q=foo');
		expect(url).toContain('limit=5');
	});
});

describe('sendToKindle', () => {
	it('calls POST /api/v1/books/:id/send with empty body when no email', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ message: 'sent', to: 'user@kindle.com' }));

		const result = await sendToKindle(10);

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/books/10/send');
		expect(opts.method).toBe('POST');
		expect(JSON.parse(opts.body)).toEqual({});
		expect(result.message).toBe('sent');
	});

	it('calls POST with email in body when provided', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ message: 'sent', to: 'custom@kindle.com' }));

		await sendToKindle(10, 'custom@kindle.com');

		const body = JSON.parse(fetchMock.mock.calls[0][1].body);
		expect(body).toEqual({ email: 'custom@kindle.com' });
	});
});

// =============== Users endpoints ===============

describe('listUsers', () => {
	it('calls GET /api/v1/users', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ users: [] }));

		const result = await listUsers();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/users');
		expect(result.users).toEqual([]);
	});
});

describe('getUser', () => {
	it('calls GET /api/v1/users/:id', async () => {
		const mockUser = { id: 3, username: 'bob', display_name: null, email: 'bob@x.com', role: 'user', kindle_email: null };
		fetchMock.mockResolvedValueOnce(okResponse(mockUser));

		const result = await getUser(3);

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/users/3');
		expect(result.username).toBe('bob');
	});
});

describe('createInvite', () => {
	it('calls POST /api/v1/users/invite', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ invite_token: 'tok', invite_url: 'https://example.com/inv/tok' }));

		const result = await createInvite();

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/users/invite');
		expect(opts.method).toBe('POST');
		expect(result.invite_token).toBe('tok');
	});
});

describe('updateUser', () => {
	it('calls PUT /api/v1/users/:id with data', async () => {
		const mockUser = { id: 2, username: 'alice', display_name: 'Alice', email: 'alice@x.com', role: 'user', kindle_email: null };
		fetchMock.mockResolvedValueOnce(okResponse(mockUser));

		const result = await updateUser(2, { display_name: 'Alice', email: 'alice@x.com' });

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/users/2');
		expect(opts.method).toBe('PUT');
		expect(JSON.parse(opts.body)).toEqual({ display_name: 'Alice', email: 'alice@x.com' });
		expect(result.display_name).toBe('Alice');
	});
});

describe('revokeUser', () => {
	it('calls DELETE /api/v1/users/:id', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ message: 'revoked', username: 'bob' }));

		const result = await revokeUser(3);

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/users/3');
		expect(opts.method).toBe('DELETE');
		expect(result.message).toBe('revoked');
	});
});

// =============== Admin endpoints ===============

describe('getSettings', () => {
	it('calls GET /api/v1/admin/settings', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ settings: { key: 'value' } }));

		const result = await getSettings();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/admin/settings');
		expect(result.settings).toEqual({ key: 'value' });
	});
});

describe('updateSettings', () => {
	it('calls PUT /api/v1/admin/settings with body', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ message: 'ok', updated: ['key'] }));

		const result = await updateSettings({ key: 'newval' });

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/admin/settings');
		expect(opts.method).toBe('PUT');
		expect(JSON.parse(opts.body)).toEqual({ key: 'newval' });
		expect(result.updated).toEqual(['key']);
	});
});

describe('getLibraryInfo', () => {
	it('calls GET /api/v1/admin/library-info', async () => {
		const info = {
			library_path: '/books',
			total_books: 100,
			available_books: 95,
			missing_books: 5,
			total_authors: 20,
			format_breakdown: [{ format: 'epub', count: 80 }]
		};
		fetchMock.mockResolvedValueOnce(okResponse(info));

		const result = await getLibraryInfo();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/admin/library-info');
		expect(result.total_books).toBe(100);
		expect(result.format_breakdown[0].format).toBe('epub');
	});
});

describe('triggerScan', () => {
	it('calls POST /api/v1/library/scan', async () => {
		const scanResult = { imported: 5, updated: 2, skipped: 10, total_scanned: 17, metadata_queued: 5 };
		fetchMock.mockResolvedValueOnce(okResponse(scanResult));

		const result = await triggerScan();

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/library/scan');
		expect(opts.method).toBe('POST');
		expect(result.imported).toBe(5);
	});
});

describe('rebuildIndex', () => {
	it('calls POST /api/v1/library/reindex', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ indexed: 100 }));

		const result = await rebuildIndex();

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/library/reindex');
		expect(opts.method).toBe('POST');
		expect(result.indexed).toBe(100);
	});
});

describe('getJobs', () => {
	it('calls GET /api/v1/admin/jobs', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ jobs: [{ name: 'scan' }] }));

		const result = await getJobs();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/admin/jobs');
		expect(result.jobs).toHaveLength(1);
	});
});

describe('triggerJob', () => {
	it('calls POST /api/v1/admin/jobs/:name/run', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ ok: true }));

		await triggerJob('scan');

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/admin/jobs/scan/run');
		expect(opts.method).toBe('POST');
	});
});

describe('getJobRuns', () => {
	it('calls GET /api/v1/admin/jobs/:name/runs with default limit', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ runs: [] }));

		await getJobRuns('scan');

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/admin/jobs/scan/runs?limit=10');
	});

	it('calls with custom limit', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ runs: [] }));

		await getJobRuns('scan', 25);

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/admin/jobs/scan/runs?limit=25');
	});
});

describe('updateJobCadence', () => {
	it('calls PUT /api/v1/admin/jobs/:name/cadence', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ ok: true }));

		await updateJobCadence('scan', 3600);

		const [url, opts] = fetchMock.mock.calls[0];
		expect(url).toBe('/api/v1/admin/jobs/scan/cadence');
		expect(opts.method).toBe('PUT');
		expect(JSON.parse(opts.body)).toEqual({ seconds: 3600 });
	});
});

describe('getAuditLog', () => {
	it('calls GET /api/v1/admin/audit-log with no params', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ entries: [], total: 0 }));

		const result = await getAuditLog();

		expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/admin/audit-log');
		expect(result.total).toBe(0);
	});

	it('includes query params when provided', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ entries: [], total: 0 }));

		await getAuditLog({ action: 'login', user_id: 1, limit: 50, offset: 10 });

		const url = fetchMock.mock.calls[0][0] as string;
		expect(url).toContain('action=login');
		expect(url).toContain('user_id=1');
		expect(url).toContain('limit=50');
		expect(url).toContain('offset=10');
	});

	it('omits undefined params', async () => {
		fetchMock.mockResolvedValueOnce(okResponse({ entries: [], total: 0 }));

		await getAuditLog({ action: 'login' });

		const url = fetchMock.mock.calls[0][0] as string;
		expect(url).toContain('action=login');
		expect(url).not.toContain('user_id');
		expect(url).not.toContain('limit');
	});
});

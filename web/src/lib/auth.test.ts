import { describe, it, expect, vi, beforeEach } from 'vitest';
import { initAuth, setAuth, clearAuth, getAuth } from './auth.svelte';

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

const mockUser = {
	id: 1,
	username: 'testuser',
	display_name: null,
	email: 'test@example.com',
	role: 'admin' as const,
	kindle_email: null
};

const regularUser = {
	id: 2,
	username: 'regular',
	display_name: 'Regular User',
	email: 'regular@example.com',
	role: 'user' as const,
	kindle_email: 'regular@kindle.com'
};

beforeEach(() => {
	localStorageMock.getItem.mockClear();
	localStorageMock.setItem.mockClear();
	localStorageMock.removeItem.mockClear();
	localStorageMock.clear.mockClear();
	for (const k of Object.keys(store)) delete store[k];
	// Reset auth state by calling clearAuth
	clearAuth();
});

describe('initAuth', () => {
	it('returns not logged in when localStorage is empty', () => {
		initAuth();
		const auth = getAuth();
		expect(auth.isLoggedIn).toBe(false);
		expect(auth.user).toBeNull();
		expect(auth.token).toBeNull();
		expect(auth.isAdmin).toBe(false);
	});

	it('restores user and token from localStorage', () => {
		store['token'] = 'stored-token';
		store['user'] = JSON.stringify(mockUser);

		initAuth();
		const auth = getAuth();

		expect(auth.isLoggedIn).toBe(true);
		expect(auth.token).toBe('stored-token');
		expect(auth.user?.username).toBe('testuser');
	});

	it('resets state if user JSON is invalid', () => {
		store['token'] = 'stored-token';
		store['user'] = 'not-valid-json{{{';

		initAuth();
		const auth = getAuth();

		expect(auth.isLoggedIn).toBe(false);
		expect(auth.user).toBeNull();
		expect(auth.token).toBeNull();
	});

	it('does not restore if only token exists (no user)', () => {
		store['token'] = 'orphan-token';
		// no 'user' key in store

		initAuth();
		const auth = getAuth();

		// token is set from localStorage, but stored (user) is null, so the if-block is skipped
		expect(auth.token).toBe('orphan-token');
		expect(auth.user).toBeNull();
		// isLoggedIn requires both token AND user
		expect(auth.isLoggedIn).toBe(false);
	});

	it('does not restore if only user exists (no token)', () => {
		store['user'] = JSON.stringify(mockUser);
		// no 'token' key

		initAuth();
		const auth = getAuth();

		expect(auth.token).toBeNull();
		expect(auth.user).toBeNull();
		expect(auth.isLoggedIn).toBe(false);
	});
});

describe('setAuth', () => {
	it('stores token and user in state and localStorage', () => {
		setAuth('new-token', mockUser);

		const auth = getAuth();
		expect(auth.isLoggedIn).toBe(true);
		expect(auth.token).toBe('new-token');
		expect(auth.user?.username).toBe('testuser');
		expect(auth.user?.email).toBe('test@example.com');

		expect(localStorageMock.setItem).toHaveBeenCalledWith('token', 'new-token');
		expect(localStorageMock.setItem).toHaveBeenCalledWith('user', JSON.stringify(mockUser));
	});

	it('overwrites previous auth state', () => {
		setAuth('token1', mockUser);
		expect(getAuth().user?.username).toBe('testuser');

		setAuth('token2', regularUser);
		expect(getAuth().user?.username).toBe('regular');
		expect(getAuth().token).toBe('token2');
	});

	it('sets isAdmin true for admin role', () => {
		setAuth('tok', mockUser);
		expect(getAuth().isAdmin).toBe(true);
	});

	it('sets isAdmin false for non-admin role', () => {
		setAuth('tok', regularUser);
		expect(getAuth().isAdmin).toBe(false);
	});
});

describe('clearAuth', () => {
	it('resets state to logged out', () => {
		setAuth('tok', mockUser);
		expect(getAuth().isLoggedIn).toBe(true);

		clearAuth();

		const auth = getAuth();
		expect(auth.isLoggedIn).toBe(false);
		expect(auth.user).toBeNull();
		expect(auth.token).toBeNull();
		expect(auth.isAdmin).toBe(false);
	});

	it('removes token and user from localStorage', () => {
		setAuth('tok', mockUser);
		localStorageMock.removeItem.mockClear();

		clearAuth();

		expect(localStorageMock.removeItem).toHaveBeenCalledWith('token');
		expect(localStorageMock.removeItem).toHaveBeenCalledWith('user');
	});
});

describe('getAuth', () => {
	it('returns reactive getters', () => {
		const auth = getAuth();

		// Initially not logged in
		expect(auth.isLoggedIn).toBe(false);

		// After setting auth, same object reflects new state
		setAuth('tok', mockUser);
		expect(auth.isLoggedIn).toBe(true);
		expect(auth.user?.username).toBe('testuser');

		// After clearing, same object reflects cleared state
		clearAuth();
		expect(auth.isLoggedIn).toBe(false);
	});

	it('returns isAdmin based on user role', () => {
		setAuth('tok', { ...mockUser, role: 'admin' });
		expect(getAuth().isAdmin).toBe(true);

		setAuth('tok', { ...mockUser, role: 'user' });
		expect(getAuth().isAdmin).toBe(false);
	});

	it('returns display_name and kindle_email from user', () => {
		setAuth('tok', regularUser);
		const auth = getAuth();
		expect(auth.user?.display_name).toBe('Regular User');
		expect(auth.user?.kindle_email).toBe('regular@kindle.com');
	});
});

import type { User } from './api';

let _user: User | null = $state(null);
let _token: string | null = $state(null);

export function initAuth() {
	const stored = localStorage.getItem('user');
	_token = localStorage.getItem('token');
	if (stored && _token) {
		try {
			_user = JSON.parse(stored);
		} catch {
			_user = null;
			_token = null;
		}
	}
}

export function setAuth(token: string, user: User) {
	_token = token;
	_user = user;
	localStorage.setItem('token', token);
	localStorage.setItem('user', JSON.stringify(user));
}

export function clearAuth() {
	_token = null;
	_user = null;
	localStorage.removeItem('token');
	localStorage.removeItem('user');
}

export function getAuth() {
	return {
		get user() { return _user; },
		get token() { return _token; },
		get isLoggedIn() { return !!_token && !!_user; },
		get isAdmin() { return _user?.role === 'admin'; }
	};
}

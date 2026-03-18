const BASE = '/api/v1';

function getToken(): string | null {
	return localStorage.getItem('token');
}

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
	const token = getToken();
	const headers: Record<string, string> = {
		'Content-Type': 'application/json',
		...(options.headers as Record<string, string>)
	};
	if (token) {
		headers['Authorization'] = `Bearer ${token}`;
	}

	const res = await fetch(`${BASE}${path}`, { ...options, headers });

	if (res.status === 401) {
		localStorage.removeItem('token');
		localStorage.removeItem('user');
		window.location.href = '/login';
		throw new Error('Unauthorized');
	}

	if (!res.ok) {
		const body = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(body.error || body.message || res.statusText);
	}

	return res.json();
}

function get<T>(path: string): Promise<T> {
	return request<T>(path);
}

function post<T>(path: string, body?: unknown): Promise<T> {
	return request<T>(path, {
		method: 'POST',
		body: body ? JSON.stringify(body) : undefined
	});
}

function put<T>(path: string, body?: unknown): Promise<T> {
	return request<T>(path, {
		method: 'PUT',
		body: body ? JSON.stringify(body) : undefined
	});
}

function del<T>(path: string): Promise<T> {
	return request<T>(path, { method: 'DELETE' });
}

// --- Auth ---

export function checkSetup(): Promise<{ setup_complete: boolean }> {
	return get('/setup/status');
}

export function setup(data: { username: string; email: string; password: string; library_path?: string }) {
	return post<{ message: string; username: string }>('/setup', data);
}

export function login(username: string, password: string) {
	return post<{ token: string; user: User }>('/auth/login', { username, password });
}

export function registerWithInvite(token: string, data: { username: string; email: string; password: string }) {
	return post<{ message: string; username: string }>(`/users/register/${token}`, data);
}

export function getMe() {
	return get<User>('/auth/me');
}

export function getOidcStatus() {
	return get<{ enabled: boolean; provider_name?: string }>('/auth/oidc/status');
}

export function getOidcAuthorize() {
	return get<{ url: string }>('/auth/oidc/authorize');
}

// --- Books ---

export interface Book {
	id: number;
	filename: string;
	file_format: string;
	file_size_bytes: number;
	added_at: string;
	title: string | null;
	subtitle: string | null;
	has_cover: boolean;
	series_name: string | null;
	series_number: number | null;
	authors: string[];
	tags: string[];
}

export interface BookDetail {
	book: {
		id: number;
		filename: string;
		file_hash: string;
		file_format: string;
		file_size_bytes: number;
		added_at: string;
		last_seen_at: string;
		missing: boolean;
	};
	metadata: {
		title: string | null;
		subtitle: string | null;
		description: string | null;
		publisher: string | null;
		published_date: string | null;
		page_count: number | null;
		language: string | null;
		isbn_10: string | null;
		isbn_13: string | null;
		series_name: string | null;
		series_number: number | null;
		has_cover: boolean;
		metadata_source: string | null;
	} | null;
	authors: string[];
	tags: string[];
}

export function listBooks(params?: {
	sort?: string;
	limit?: number;
	offset?: number;
	author?: string;
	tag?: string;
	format?: string;
}) {
	const qs = new URLSearchParams();
	if (params?.sort) qs.set('sort', params.sort);
	if (params?.limit) qs.set('limit', String(params.limit));
	if (params?.offset) qs.set('offset', String(params.offset));
	if (params?.author) qs.set('author', params.author);
	if (params?.tag) qs.set('tag', params.tag);
	if (params?.format) qs.set('format', params.format);
	const query = qs.toString();
	return get<{ books: Book[]; total: number; limit: number; offset: number }>(
		`/books${query ? '?' + query : ''}`
	);
}

export function getBook(id: number) {
	return get<BookDetail>(`/books/${id}`);
}

export interface FilterItem {
	name: string;
	book_count: number;
}

export function listAuthors() {
	return get<{ authors: FilterItem[] }>('/authors');
}

export function listTags() {
	return get<{ tags: FilterItem[] }>('/tags');
}

export function listFormats() {
	return get<{ formats: FilterItem[] }>('/formats');
}

export function searchBooks(q: string, limit?: number) {
	const qs = new URLSearchParams({ q });
	if (limit) qs.set('limit', String(limit));
	return get<{ books: Book[]; query: string }>(`/books/search?${qs}`);
}

export function coverUrl(bookId: number): string {
	return `${BASE}/books/${bookId}/cover`;
}

export function downloadUrl(bookId: number): string {
	return `${BASE}/books/${bookId}/download`;
}

export function sendToKindle(bookId: number, email?: string) {
	return post<{ message: string; to: string }>(`/books/${bookId}/send`, email ? { email } : {});
}

// --- Users ---

export interface User {
	id: number;
	username: string;
	display_name: string | null;
	email: string;
	role: string;
	kindle_email: string | null;
	created_at?: string;
}

export function listUsers() {
	return get<{ users: User[] }>('/users');
}

export function getUser(id: number) {
	return get<User>(`/users/${id}`);
}

export function createInvite() {
	return post<{ invite_token: string; invite_url: string }>('/users/invite');
}

export function updateUser(id: number, data: {
	display_name?: string;
	email?: string;
	kindle_email?: string;
	current_password?: string;
	new_password?: string;
	role?: string;
}) {
	return put<User>(`/users/${id}`, data);
}

export function revokeUser(id: number) {
	return del<{ message: string; username: string }>(`/users/${id}`);
}

// --- Admin ---

export function getSettings() {
	return get<{ settings: Record<string, string>; env_locked: string[] }>('/admin/settings');
}

export function updateSettings(settings: Record<string, string>) {
	return put<{ message: string; updated: string[] }>('/admin/settings', settings);
}

export function getLibraryInfo() {
	return get<{
		library_path: string | null;
		total_books: number;
		available_books: number;
		missing_books: number;
		total_authors: number;
		format_breakdown: { format: string; count: number }[];
	}>('/admin/library-info');
}

export function triggerScan() {
	return post<{
		imported: number;
		updated: number;
		skipped: number;
		total_scanned: number;
		metadata_queued: number;
	}>('/library/scan');
}

export function rebuildIndex() {
	return post<{ indexed: number }>('/library/reindex');
}

export function getJobs() {
	return get<{ jobs: any[] }>('/admin/jobs');
}

export function triggerJob(name: string) {
	return post<any>(`/admin/jobs/${name}/run`);
}

export function getJobRuns(name: string, limit = 10) {
	return get<{ runs: any[] }>(`/admin/jobs/${name}/runs?limit=${limit}`);
}

export function updateJobCadence(name: string, seconds: number) {
	return put<any>(`/admin/jobs/${name}/cadence`, { seconds });
}

// --- Providers ---

export interface ProviderInfo {
	name: string;
	enabled: boolean;
	order: number;
	requires_key: boolean;
	key_configured: boolean;
}

export function getProviders() {
	return get<{ providers: ProviderInfo[] }>('/admin/providers');
}

export function updateProviders(providers: string[]) {
	return put<{ message: string }>('/admin/providers', { providers });
}

export function testHardcoverKey(apiKey: string) {
	return post<{ message: string }>('/admin/providers/test-hardcover', { api_key: apiKey });
}

export function resetProvider(name: string) {
	return post<{ message: string; cleared: number }>(`/admin/providers/${name}/reset`);
}

export function getAuditLog(params?: { action?: string; user_id?: number; limit?: number; offset?: number }) {
	const qs = new URLSearchParams();
	if (params?.action) qs.set('action', params.action);
	if (params?.user_id) qs.set('user_id', String(params.user_id));
	if (params?.limit) qs.set('limit', String(params.limit));
	if (params?.offset) qs.set('offset', String(params.offset));
	const query = qs.toString();
	return get<{
		entries: {
			id: number;
			user_id: number | null;
			username: string | null;
			action: string;
			detail: string | null;
			ip_address: string | null;
			created_at: string;
		}[];
		total: number;
	}>(`/admin/audit-log${query ? '?' + query : ''}`);
}

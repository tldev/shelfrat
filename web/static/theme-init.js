(function () {
	var t = localStorage.getItem('theme');
	if (
		t === 'dark' ||
		((!t || t === 'system') && matchMedia('(prefers-color-scheme: dark)').matches)
	)
		document.documentElement.classList.add('dark');
})();

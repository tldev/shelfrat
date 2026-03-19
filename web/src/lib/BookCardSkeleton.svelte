<div class="card">
	<div class="cover skeleton"></div>
	<div class="info">
		<div class="title skeleton"></div>
		<div class="author skeleton"></div>
		<div class="badge-skel skeleton"></div>
	</div>
</div>

<style>
	.card {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		min-width: 0;
	}

	.cover {
		aspect-ratio: 2 / 3;
		width: 100%;
	}

	.info {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.title {
		height: 0.85rem;
		width: 85%;
	}

	.author {
		height: 0.7rem;
		width: 60%;
	}

	.badge-skel {
		height: 0.65rem;
		width: 2.5rem;
		margin-top: 0.1rem;
	}
</style>

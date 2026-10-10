const urlInput = document.getElementById('urlInput');
const fetchBtn = document.getElementById('fetchBtn');
const resultsDiv = document.getElementById('results');

fetchBtn.addEventListener('click', async () => {
    const url = urlInput.value.trim();
    if (!url) return;

    resultsDiv.classList.remove('hidden');
    resultsDiv.innerHTML = '<span class="text-cyan-400">Loading...</span>';

    try {
        const response = await fetch('/api/extract', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ url })
        });
        const data = await response.json();
        resultsDiv.innerHTML = `<pre class="text-green-400">${JSON.stringify(data, null, 2)}</pre>`;
    } catch (err) {
        resultsDiv.innerHTML = `<span class="text-red-400">Error: ${err.message}</span>`;
    }
});

const urlInput = document.getElementById('urlInput');
const fetchBtn = document.getElementById('fetchBtn');
const resultsDiv = document.getElementById('results');

function size_in_mb(bytes) {
    if (bytes === null || bytes === undefined) return 'Unknown Size';
    return (bytes / 1048576).toFixed(1) + ' MB';
}

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
        
        if (data.status === 'error') {
            resultsDiv.innerHTML = '<span class="text-red-400 font-semibold">\nERROR: ' + data.message + '\n</span>';
        } else if (data.status === 'success') {
            let html = `<h2 class="text-cyan-400 font-bold mb-4 text-xl">${data.data.title}</h2>`;
            let formats = data.data.formats.reverse();
            for (const format of formats) {
                const tag = format.vcodec === "none" ? "[AUDIO]" : "[VIDEO]";
                const size = size_in_mb(format.filesize);
                html += `
                <div class="flex justify-between items-center bg-gray-800 p-3 rounded mb-2 border border-gray-700">
                    <div>
                        <span class="text-gray-400 font-bold mr-2">${tag}</span>
                        <span class="text-white mr-2">${format.resolution}</span>
                        <span class="text-gray-500 text-xs uppercase">${format.ext}</span>
                        <span class="text-cyan-500 text-sm ml-3">${size}</span>
                    </div>
                    <button id="btn-${format.format_id}" onclick="startDownload('${format.format_id}', 'btn-${format.format_id}')" class="bg-cyan-600 hover:bg-cyan-500 text-white font-semibold py-1 px-3 rounded shadow-lg shadow-cyan-900/50 transition duration-200">
                        Download
                    </button>
                </div>`;
            }
            resultsDiv.innerHTML = html;
        }
    } catch (err) {
        resultsDiv.innerHTML = `<span class="text-red-400 font-semibold">\nERROR: ${err.message}\n</span>`;
    }
});

window.startDownload = async function(format_id, btnId) {
    const url = document.getElementById('urlInput').value.trim();
    const btn = document.getElementById(btnId);
    if (!url) return;

    btn.disabled = true;
    btn.innerHTML = 'Starting...';

    try {
        const res = await fetch('/api/download', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ url, format_id })
        });
        const data = await res.json();
        if (data.status === 'success') {
            const taskId = data.task_id;
            const poll = setInterval(async () => {
                try {
                    const statusRes = await fetch(`/api/status?task_id=${taskId}`);
                    const statusData = await statusRes.json();
                    if (statusData.progress === 'Completed') {
                        btn.innerHTML = 'SUCCESS: Done!';
                        btn.classList.replace('bg-cyan-600', 'bg-green-600');
                        btn.classList.replace('hover:bg-cyan-500', 'hover:bg-green-500');
                        clearInterval(poll);
                    } else if (statusData.progress.startsWith('Error')) {
                        btn.innerHTML = 'ERROR';
                        btn.classList.replace('bg-cyan-600', 'bg-red-600');
                        btn.classList.replace('hover:bg-cyan-500', 'hover:bg-red-500');
                        clearInterval(poll);
                        console.error(statusData.progress);
                    } else {
                        btn.innerHTML = statusData.progress;
                    }
                } catch (e) {
                    btn.innerHTML = 'ERROR';
                    clearInterval(poll);
                }
            }, 2000);
        }
    } catch (e) {
        btn.innerHTML = 'ERROR';
    }
};

const urlInput = document.getElementById('urlInput');
const fetchBtn = document.getElementById('fetchBtn');
const resultsDiv = document.getElementById('results');

const settingsModal = document.getElementById('settingsModal');
const openSettingsBtn = document.getElementById('openSettingsBtn');
const cancelConfigBtn = document.getElementById('cancelConfigBtn');
const saveConfigBtn = document.getElementById('saveConfigBtn');
const configPath = document.getElementById('configPath');
const configQuality = document.getElementById('configQuality');

function size_in_mb(bytes) {
    if (bytes === null || bytes === undefined) return 'Unknown Size';
    return (bytes / 1048576).toFixed(1) + ' MB';
}

fetchBtn.addEventListener('click', async () => {
    const url = urlInput.value.trim();
    if (!url) return;

    resultsDiv.classList.remove('hidden');
    resultsDiv.innerHTML = '<span class="text-cyan-400 font-medium">Loading media details...</span>';

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
                const isAudio = format.vcodec === "none";
                const tag = isAudio ? "[AUDIO]" : "[VIDEO]";
                const rawSize = size_in_mb(format.filesize);
                const sizeHtml = rawSize === 'Unknown Size'
                    ? '<span class="text-gray-600 text-xs ml-3 font-semibold">N/A</span>'
                    : `<span class="text-cyan-400 text-sm ml-3 font-medium">${rawSize}</span>`;

                html += `
                <div class="flex items-center justify-between bg-gray-800/90 hover:bg-gray-800 p-3 rounded-lg mb-2 border border-gray-700/80 transition duration-150">
                    <div class="flex items-center flex-wrap gap-2">
                        <span class="text-xs font-bold px-2 py-0.5 rounded ${isAudio ? 'bg-purple-900/60 text-purple-300 border border-purple-700/60' : 'bg-blue-900/60 text-blue-300 border border-blue-700/60'}">${tag}</span>
                        <span class="text-white font-medium text-sm">${format.resolution}</span>
                        <span class="text-gray-400 text-xs uppercase px-1.5 py-0.5 rounded bg-gray-900 border border-gray-700">${format.ext}</span>
                        ${sizeHtml}
                    </div>
                    <button id="btn-${format.format_id}" onclick="startDownload('${format.format_id}', 'btn-${format.format_id}')" class="bg-cyan-600 hover:bg-cyan-500 text-white font-semibold py-1.5 px-4 rounded-lg shadow-md shadow-cyan-900/40 transition duration-200 text-sm shrink-0 ml-4">
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

window.startDownload = async function (format_id, btnId) {
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

openSettingsBtn.addEventListener('click', async () => {
    try {
        const res = await fetch('/api/config');
        const data = await res.json();
        if (data.status === 'success') {
            configPath.value = data.data.download_path || '';
            configQuality.value = data.data.default_quality || '';
        }
    } catch (e) {
        console.error('Failed to load config', e);
    }
    settingsModal.classList.remove('hidden');
});

cancelConfigBtn.addEventListener('click', () => {
    settingsModal.classList.add('hidden');
});

saveConfigBtn.addEventListener('click', async () => {
    saveConfigBtn.disabled = true;
    saveConfigBtn.textContent = 'Saving...';
    try {
        const res = await fetch('/api/config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                download_path: configPath.value.trim(),
                default_quality: configQuality.value.trim()
            })
        });
        const data = await res.json();
        if (data.status === 'success') {
            settingsModal.classList.add('hidden');
        } else {
            alert('Failed to save settings: ' + (data.message || 'Unknown error'));
        }
    } catch (e) {
        alert('Failed to save settings: ' + e.message);
    } finally {
        saveConfigBtn.disabled = false;
        saveConfigBtn.textContent = 'Save';
    }
});

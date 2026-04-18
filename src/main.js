const invoke = window.__TAURI__.core.invoke;
const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;
const { saveWindowState, StateFlags, restoreStateCurrent } = window.__TAURI__.windowState;


const appWindow = getCurrentWindow();


restoreStateCurrent(StateFlags.POSITION);
restoreStateCurrent(StateFlags.SIZE)
listen("show-hide", async () => {
    const isVisible = await appWindow.isVisible();

    if (isVisible) {
        saveWindowState(StateFlags.ALL);
        await appWindow.hide();
    } else {
        restoreStateCurrent(StateFlags.ALL);
        await appWindow.setAlwaysOnTop(true);
        await appWindow.show();
        await appWindow.setFocus();
    }
});


document.getElementById('titlebar-left')?.addEventListener('mousedown', (e) => {
    if (e.buttons === 1) {
        if (e.detail === 2) {
            // Double-click = hide after saving state
            try {
                saveWindowState(StateFlags.ALL);
            } catch (err) {
                console.error("Failed to save window state before hide:", err);
            }
            appWindow.hide();
        } else {
            appWindow.startDragging();
        }
    }
});

document.getElementById('titlebar-right')?.addEventListener('mousedown', (e) => {
    if (e.buttons === 1) {
        if (e.detail === 2) {
            // Double-click = quit after saving state
            try {
                saveWindowState(StateFlags.ALL);
            } catch (err) {
                console.error("Failed to save window state before quit:", err);
            }
            invoke("quit");
        } else {
            appWindow.startDragging();
        }
    }
});

document.addEventListener("DOMContentLoaded", () => {
    const playBtn = document.getElementById("playBtn");
    const nextBtn = document.getElementById("nextBtn");
    const prevBtn = document.getElementById("prevBtn");

    const statusText = document.getElementById("statusText");

    const thumbnailImg = document.getElementById("thumbnail");
    const seekSlider = document.getElementById("seekSlider");

    let currentTitle = "";
    let isPaused = true;
    let totalDuration = 0;

    let currentPosition = 0;
    
    let isScrubbing = false;

    seekSlider.addEventListener("mousedown", () => {
        isScrubbing = true;
    });

    seekSlider.addEventListener("mouseup", () => {
        isScrubbing = false;
        const posMs = parseInt(seekSlider.value, 10) * 1000;
        invoke("set_position_command", { posMs });
    });
    
    listen("next_song", (event) => {
        pollTitle();
        pollPosition();
    });

    seekSlider.addEventListener("input", () => {
        const posSec = parseInt(seekSlider.value, 10);
        const posMs = posSec * 1000;
        if (posMs <= 100) {
            posMs = 100;
        }
        invoke("set_position_command", { posMs });
    });

    document.addEventListener("keydown", (event) => {
        if (event.key === "ArrowUp") {
            invoke("volume_up");
        } else if (event.key === "ArrowDown") {
            invoke("volume_down");
        }
    });

    document.addEventListener("keyup", (event) => {

        switch (event.code) {
            case "Space":
                event.preventDefault();
                playandpause();
        }
        if (event.key === "ArrowRight") {
            invoke("next");
            currentPosition = 0;
            pollTitle();
        } else if (event.key === "ArrowLeft") {
            invoke("previous");
            currentPosition = 0;
            pollTitle();
        }
    });

    listen('tray-play-pause', () => {
        // Llama función play/pause
        playandpause();
    });
    listen('tray-next', () => {
        // Llama función siguiente
        invoke("next");
        pollTitle();
    });
    listen('tray-previous', () => {
        // Llama función anterior
        invoke("previous");
        pollTitle();
    });

    function playandpause() {
        if (isPaused) {
            invoke("play");
        } else {
            invoke("pause");
        }
        isPaused = !isPaused;
        playBtn.textContent = isPaused ? "▶️" : "⏸️";
        currentPosition = parseInt(seekSlider.value, 10);
        updateStatusText();
    }

    playBtn.addEventListener("click", () => {
        playandpause();
    });

    nextBtn.addEventListener("click", () => {
        invoke("next");
        currentPosition = 0;
        pollTitle();
    });

    prevBtn.addEventListener("click", () => {
        invoke("previous");
        currentPosition = 0;
        pollTitle();
    });

    function updateStatusText() {
        statusText.textContent = `${currentTitle}`;

        updateSliderDisplay();
    }
    function updateSliderDisplay() {
        seekSlider.max = totalDuration;
        if (!isScrubbing) {
            seekSlider.value = currentPosition;
        }
    }
    function pollTitle() {
        invoke("get_title")
            .then(([title, paused, id, duration]) => {
                console.log("Title response:", title, paused, id, duration);
                currentTitle = title;
                isPaused = paused;
                totalDuration = duration;
                currentPosition = 0;
                updateStatusText();

                loadThumbnail(id);
            })
            .catch((err) => {
                console.error("Failed to fetch title/status", err);
                statusText.textContent = "⚠️ Error retrieving status";
            });
    }

    function pollPosition() {
        invoke("get_pos")
            .then((position) => {
                currentPosition = position;
                updateStatusText();
            })
            .catch((err) => {
                console.error("Failed to fetch position", err);
            });
        updateSliderDisplay();
    }

    function loadThumbnail(id) {
        console.log("Loading thumbnail for ID:", id);

        invoke("get_thumbnail_base64", { id })
            .then((dataUri) => {
                thumbnailImg.src = dataUri;
                thumbnailImg.alt = "Thumbnail for ID " + id;
            })
            .catch((err) => {
                console.error("Failed to load thumbnail:", err);
                thumbnailImg.alt = "Failed to load image";
            });
    }

    // Initial calls
    pollTitle();
    pollPosition();

    setInterval(pollPosition, 1000);
});

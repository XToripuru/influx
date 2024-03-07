const socket = new WebSocket("ws://localhost:80/connect");

const loading = document.getElementById("loading");
const drop = document.getElementById("drop");

let receive = {
    "Link": (event) => {
        drop.classList.add("hidden");

        const link = document.getElementById("link");

        link.classList.remove("hidden");

        link.children[0].href = `d/${event.link}`;
        link.children[0].textContent = `localhost/d/${event.link}`;

        navigator.clipboard.writeText("localhost/d/" + event.link);
    },
};

socket.addEventListener("message", (event) => {
    let parsed = JSON.parse(event.data);

    let type = Object.keys(parsed)[0];
    
    if(type == 0) {
        receive[parsed]();
    } else {
        let value = Object.values(parsed)[0];
        receive[type](value);
    }
});

const CHUNK = 64 * 1024;

const queue = [];
let process = null;
let width = 0.0;

const updateLoading = () => {
    width += (process.percent - width) * 0.1;
    loading.style.width = `${width}%`;
    if(width < 99.9) {
        setTimeout(updateLoading, 10);
    } else {
        loading.style.width = `100%`;
    }
}

const write = () => {
    let k = 0;
    let start = process.k * CHUNK;
    let end = start + CHUNK;
    if(end >= process.task.file.size) {
        end = process.task.file.size;
        process.done = true;
    }
    socket.send(process.bytes.slice(start, end));
    process.percent = 100.0 * end / process.task.file.size;
    process.k += 1;

    if(!process.done) {
        setTimeout(write, 10);
    }
}

const load = () => {
    if(queue.length == 0) return;
    const [task] = queue.splice(0, 1);

    socket.send(JSON.stringify({
        "File": {
            "file": task.file.name,
            "size": task.file.size
        }
    }));

    const reader = new FileReader();

    reader.addEventListener("load", (event) => {
        const arrayBuffer = event.target.result;
        const bytes = new Uint8Array(arrayBuffer);

        process = {
            task: task,
            bytes: bytes,
            k: 0,
            percent: 0.0,
            done: false
        };

        setTimeout(write, 100);
        setTimeout(updateLoading, 100);
    });

    reader.readAsArrayBuffer(task.file);
};

drop.addEventListener("drop", (event) => {
    event.preventDefault();

    [...event.dataTransfer.files].forEach((file, i) => {
        queue.push({
            file: file
        });
    });

    setTimeout(load, 100);
});

drop.addEventListener("dragover", (event) => {
    event.stopPropagation();
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
});

drop.addEventListener("dragenter", (event) => {
    event.preventDefault();
    drop.classList.add("dragover");
});

drop.addEventListener("dragleave", (event) => {
    event.preventDefault();
    drop.classList.remove("dragover");
});
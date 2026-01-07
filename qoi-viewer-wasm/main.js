async function main() {
    const blob = await fetch("qoi_rs.wasm");
    if (!blob.ok) throw new Error(`Failed to fetch WASM: ${blob.statusText}`);
    const module = await WebAssembly.instantiateStreaming(blob);
    const wasm = module.instance.exports;

    const suz = await fetch("../encoded.qoi");
    if (!suz.ok) throw new Error(`Failed to fetch image: ${suz.statusText}`)
    const data = new Uint8Array(await suz.arrayBuffer());

    const alloc = new WasmPageAllocator(wasm.memory);
    const _width = alloc.u32();
    const _height = alloc.u32();
    const _image = alloc.u8(data.length);
    alloc.reserve();

    _image.view.set(data);
    const imgPtr = wasm.qoi_decode(
        _image.byteOffset,
        _image.byteLength,
        _width.byteOffset,
        _height.byteOffset);
    const width = _width.get(0);
    const height = _height.get(0);

    const imgSize = width * height * 4;
    const image = new Uint8ClampedArray(wasm.memory.buffer, imgPtr, imgSize);

    const ctx = scene.getContext('2d');
    const imageData = new ImageData(image, width, height);

    // TODO: panics
    // wasm.qoi_free(imgPtr);

    // resizing the canvas clears it so redraw
    (window.onresize = function() {
        scene.width = window.innerWidth;
        scene.height = window.innerHeight;
        ctx.putImageData(imageData, 0, 0);
    })();
};

window.onload = main;

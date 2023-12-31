const ws = new WebSocket("ws://127.0.0.1:9003");

ws.addEventListener("message", (event) => {
  console.log(event);
})

let ws;
let names = [];

let loaded = false;
const tablegrid = document.getElementById("tablegrid");

if (localStorage.getItem('kart') === null) {
  for (let i = 0; i < 16; i++) {
    names.push(i);
  }
} else {
  names = localStorage.getItem('kart').split(',');
}



console.log(names);

function scramble() {
  if (ws?.readyState == 1)
    ws.send("SCRAMBLE");
}

async function setupLayout() {
  let layout = (await fetch("./layout.txt").then(r => r.text())).trimEnd();
  layout = layout.split(/\/\*[\s\S]*?\*\//m).join(""); // Comments
  let layoutline = layout.split("\n")[0];
  let layouttokens = layout.split("");

  tablegrid.style.gridTemplateColumns = `repeat(${layoutline.length}, 1fr)`;

  for (let i = 0; i < layouttokens.length; i++) {
    let spot = document.createElement("div");
    if (layouttokens[i][0] != " ") {
      spot.classList.add("spot");
      spot.classList.add(["up", "right", "down", "left"][parseInt(layouttokens[i][0])]);
      
      let img = document.createElement("img");
      img.src = "./Bord.svg";
      spot.appendChild(img);

      spot.appendChild(document.createElement("p"));
    }

    if (layouttokens[i][0] != "\n")
      tablegrid.appendChild(spot);
  }

  if (tablegrid.innerHTML != "") loaded = true;
}

function render() {
  let spots = document.getElementsByClassName("spot");
  for (let i = 0; i < names.length; i++) {
    spots[i].getElementsByTagName("p")[0].innerText = names[i];
  }
}

// Startup 
let start = async () => {
  await setupLayout();
  render();
  ws = new WebSocket(`ws://${window.location.hostname}:9003`);
  ws.addEventListener("message", (ev) => {
    names = ev.data.split(",");
  render();
});
}
start();
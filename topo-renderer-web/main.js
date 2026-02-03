export function push_notification(notification) {
	var toast = document.getElementById("toast");
	var desc = document.getElementById("toast-desc");
	var toast_count = document.getElementById("toast-count");
	var next;
	var count = Number(toast_count.innerHTML);
	if (desc.innerHTML.trim() === "") {
		desc.innerHTML = notification;
	} else {
		next = document.createElement("span");
		next.classList.add("next");
		next.innerHTML = notification;
		toast.appendChild(next);
	}
	toast_count.innerHTML = String(count + 1);
}

/* Accessible name suggestions. A fuzzy match is never selected without consent. */
(() => {
    const input = document.getElementById("existing-user");
    const list = document.getElementById("member-matches");
    const selectedUser = document.getElementById("selected-user-id");
    if (!input || !list) return;
    const options = [...list.querySelectorAll("[data-name]")];
    const status = document.getElementById("search-status");
    const wrapper = input.closest(".name-search");
    let matches = [];
    let active = -1;

    const normalize = value => value.normalize("NFKD").replace(/\p{M}/gu, "").toLocaleLowerCase().trim();

    // Edit distance with adjacent transpositions, for small spelling errors.
    function distance(left, right) {
        const rows = Array.from({ length: left.length + 1 }, (_, i) => [i]);
        rows[0] = Array.from({ length: right.length + 1 }, (_, i) => i);
        for (let i = 1; i <= left.length; i += 1) {
            for (let j = 1; j <= right.length; j += 1) {
                rows[i][j] = Math.min(rows[i - 1][j] + 1, rows[i][j - 1] + 1, rows[i - 1][j - 1] + (left[i - 1] === right[j - 1] ? 0 : 1));
                if (i > 1 && j > 1 && left[i - 1] === right[j - 2] && left[i - 2] === right[j - 1]) {
                    rows[i][j] = Math.min(rows[i][j], rows[i - 2][j - 2] + 1);
                }
            }
        }
        return rows[left.length][right.length];
    }

    function score(name, query) {
        if (name === query) return 0;
        if (name.startsWith(query)) return 10 + name.length / 100;
        const position = name.indexOf(query);
        if (position >= 0) return 20 + position;
        let cursor = 0;
        for (const character of name) { if (character === query[cursor]) cursor += 1; }
        if (cursor === query.length) return 50 + name.length - query.length;
        if (query.length < 3) return Infinity;
        const edits = Math.min(...[name, ...name.split(/\s+/)].map(word => distance(word, query)));
        return edits <= (query.length >= 6 ? 2 : 1) ? 100 + edits : Infinity;
    }

    function highlight(index) {
        active = index;
        options.forEach(option => option.setAttribute("aria-selected", "false"));
        if (active >= 0) {
            matches[active].setAttribute("aria-selected", "true");
            input.setAttribute("aria-activedescendant", matches[active].id);
            matches[active].scrollIntoView({ block: "nearest" });
        } else { input.removeAttribute("aria-activedescendant"); }
    }

    function close() {
        list.hidden = true;
        input.setAttribute("aria-expanded", "false");
        highlight(-1);
    }

    function update() {
        const query = normalize(input.value);
        matches = options.map(option => ({ option, score: score(normalize(option.dataset.name), query) }))
            .filter(match => Number.isFinite(match.score))
            .sort((left, right) => left.score - right.score || left.option.dataset.name.localeCompare(right.option.dataset.name))
            .slice(0, 8).map(match => match.option);
        options.forEach(option => { option.hidden = true; });
        matches.forEach(option => { option.hidden = false; list.append(option); });
        list.hidden = matches.length === 0;
        input.setAttribute("aria-expanded", String(matches.length > 0));
        highlight(-1);
        status.textContent = matches.length ? `${matches.length} matching names. Select yours, then continue.` : "No matching names. Add a new member below.";
    }

    function select(option) {
        input.value = option.dataset.name;
        selectedUser.value = option.dataset.userId;
        selectedUser.disabled = false;
        input.focus({ preventScroll: true });
        close();
        status.textContent = `${input.value} selected. Choose Continue.`;
    }

    input.addEventListener("input", () => {
        selectedUser.value = "";
        selectedUser.disabled = true;
        update();
    });
    input.addEventListener("focus", update);
    input.addEventListener("keydown", event => {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            if (list.hidden) update();
            if (matches.length) highlight(event.key === "ArrowDown" ? (active + 1) % matches.length : (active <= 0 ? matches.length : active) - 1);
        } else if (event.key === "Enter" && active >= 0 && !list.hidden) {
            event.preventDefault();
            select(matches[active]);
        } else if (event.key === "Escape") { close(); }
    });
    list.addEventListener("mousedown", event => { event.preventDefault(); });
    list.addEventListener("click", event => {
        const option = event.target.closest("[data-name]");
        if (option) select(option);
    });
    wrapper.addEventListener("focusout", () => requestAnimationFrame(() => {
        if (!wrapper.contains(document.activeElement)) close();
    }));
    document.getElementById("new-member").addEventListener("toggle", event => {
        if (event.target.open) {
            const newName = document.getElementById("new-user-name");
            if (!newName.value) newName.value = input.value.trim();
        }
    });
})();

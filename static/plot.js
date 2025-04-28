async function initPlot() {

    const COLOR_BREWER = await (await fetch('/static/colorbrewer-light.json')).json();
    const COLOR_THEME = "YlGn";

    class Options {
        constructor() {
            let fromStorage = JSON.parse(localStorage.getItem('options')) || {};
            this._colorTheme = fromStorage.colorTheme || "YlGn";
            this._widthStat = Stat.getByName(fromStorage.widthStat) || Stat.SPECIES_COUNT;
            this._colorStat = Stat.getByName(fromStorage.colorStat) || Stat.MEDIAS_CROPPED_OVER_MEDIAS_DOWNLOADED;
        }

        set colorTheme(value) {
            this._colorTheme = value;
            this.save();
        }

        set widthStat(value) {
            if (value instanceof String || typeof value === "string") {
                let stat = Stat.getByName(value);
                if (stat !== null) {
                    this._widthStat = stat;
                }
            } else if (value instanceof Stat) {
                this._widthStat = value;
            }
            this.save();
        }

        set colorStat(value) {
            if (value instanceof String || typeof value === "string") {
                let stat = Stat.getByName(value);
                if (stat !== null) {
                    this._colorStat = stat;
                }
            } else if (value instanceof Stat) {
                this._colorStat = value;
            }
            this.save();
        }

        get colorTheme() {
            return this._colorTheme;
        }

        get widthStat() {
            return this._widthStat;
        }

        get colorStat() {
            return this._colorStat;
        }

        toJson() {
            return {
                colorTheme: this._colorTheme,
                widthStat: this._widthStat.name,
                colorStat: this._colorStat.name,
            };
        }

        save() {
            localStorage.setItem('options', JSON.stringify(this.toJson()));
        }
    }

    class Stat {
        static SPECIES_COUNT = new Stat("species_count");
        static MEDIAS_COUNT = new Stat("medias_count");
        static MEDIAS_DOWNLOADED_COUNT = new Stat("medias_downloaded_count");
        static MEDIAS_CROPPED_COUNT = new Stat("medias_cropped_count");
        static MEDIAS_DOWNLOADED_OVER_MEDIAS = new Stat("medias_downloaded_over_medias");
        static MEDIAS_CROPPED_OVER_MEDIAS = new Stat("medias_cropped_over_medias");
        static MEDIAS_CROPPED_OVER_MEDIAS_DOWNLOADED = new Stat("medias_cropped_over_medias_downloaded");

        static all() {
            return [
                Stat.SPECIES_COUNT,
                Stat.MEDIAS_COUNT,
                Stat.MEDIAS_DOWNLOADED_COUNT,
                Stat.MEDIAS_CROPPED_COUNT,
                Stat.MEDIAS_DOWNLOADED_OVER_MEDIAS,
                Stat.MEDIAS_CROPPED_OVER_MEDIAS,
                Stat.MEDIAS_CROPPED_OVER_MEDIAS_DOWNLOADED,
            ];
        }

        static getByName(name) {
            for (let stat of Stat.all()) {
                if (stat.name === name) {
                    return stat;
                }
            }

            return null;
        }

        constructor(name) {
            this.name = name;
        }


        get(metadata) {
            switch (this.name) {
                case Stat.MEDIAS_DOWNLOADED_OVER_MEDIAS.name:         return metadata.medias_downloaded_count / metadata.medias_count;
                case Stat.MEDIAS_CROPPED_OVER_MEDIAS.name:            return metadata.medias_cropped_count / metadata.medias_count;
                case Stat.MEDIAS_CROPPED_OVER_MEDIAS_DOWNLOADED.name: return metadata.medias_cropped_count / metadata.medias_downloaded_count;
                default:                                              return metadata[this.name];
            }
        }
    }



    let colorCounter = 0;

    function depthToTaxon(depth) {
        switch (depth) {
            case 0: return "reign";
            case 1: return "phylum";
            case 2: return "class";
            case 3: return "order";
            case 4: return "family";
            case 5: return "genus";
            case 6: return "species";
            default:
                throw new Error("taxon depth does not exist: " + depth);
        }
    }

    function taxonToDepth(taxon) {
        switch (taxon) {
            case "reign":   return 0;
            case "phylum":  return 1;
            case "class":   return 2;
            case "order":   return 3;
            case "family":  return 4;
            case "genus":   return 5;
            case "species": return 6;
            default:
                throw new Error("taxon does not exist: " + taxon);
        }
    }

    function interpolateColor(startColor, endColor, value) {
        let startR = parseInt(startColor[1] + startColor[2], 16);
        let startG = parseInt(startColor[3] + startColor[4], 16);
        let startB = parseInt(startColor[5] + startColor[6], 16);

        let endR = parseInt(endColor[1] + endColor[2], 16);
        let endG = parseInt(endColor[3] + endColor[4], 16);
        let endB = parseInt(endColor[5] + endColor[6], 16);

        let r = Math.floor((1 - value) * startR + value * endR);
        let g = Math.floor((1 - value) * startG + value * endG);
        let b = Math.floor((1 - value) * startB + value * endB);

        return `rgb(${r}, ${g}, ${b})`;
    }

    class Tree {
        constructor(name, depth = 0) {
            colorCounter++;

            this.parent = null;
            this.name = name;
            this.children = [];
            this.hovering = false;
            this.depth = depth;
        }

        prettyName() {
            if (this.name == undefined) {
                return "";
            }

            if (this.depth !== 6) {
                return this.name;
            }

            let split = this.name.replaceAll('(', '').replaceAll(')', '').split(' ');
            let authorIndex = split.map((x, index) => index > 0 && x[0] === x[0].toUpperCase()).indexOf(true);
            return split.slice(0, authorIndex).join(' ');
        }

        static fromJson(value) {
            let root = null;
            let current = null;

            for (let row of value) {
                let name = null;
                let depth = null;

                for (let index = 0; index < 7; index++) {
                    if (row[depthToTaxon(index)] === null) {
                        name = row[depthToTaxon(index - 1)]
                        depth = index - 1;
                        break;
                    }
                }

                if (name === null) {
                    // It means we're dealing with a species
                    name = row[depthToTaxon(6)];
                    depth = 6;
                }

                let tree = new Tree(name, depth);
                tree.metadata = row;

                if (root === null) {
                    root = tree;
                    current = tree;
                    continue;
                }

                tree.root = root;

                // Depth check
                while (depth !== current.depth + 1) {
                    current = current.parent;
                }

                current.appendChild(tree);
                current = tree;
            }

            return root;
        }

        appendChild() {
            for (let arg of arguments) {
                arg.parent = this;
            }
            this.children.push(...arguments);
            return this;
        }

        width(options) {
            if (options.widthStat == undefined) {
                throw new Error("Width stat is not defined");
            }
            return options.widthStat.get(this.metadata) || 1;
        }

        // Returns a float between 0 and 1
        colorValue(options) {
            if (options.colorStat == undefined) {
                throw new Error("Color stat is not defined");
            }
            return options.colorStat.get(this.metadata);
        }

        // Returns a canvas ready color from colorValue
        color(options) {
            let value = this.colorValue(options);

            if (isNaN(value)) {
                return 'white';
            }

            let startColor = COLOR_BREWER[options.colorTheme][0];
            let endColor = COLOR_BREWER[options.colorTheme][1];

            return interpolateColor(startColor, endColor, value);
        }

        log() {
            let chain = [this.name];
            let tmp = this;

            while (tmp.parent !== null) {
                chain.push(tmp.parent.name);
                tmp = tmp.parent;
            }

            console.log(chain.reverse().join(" > "));
        }
    };

    class Chart {
        constructor(parent, root) {
            if (parent instanceof HTMLElement) {
                this.parent = parent;
            } else if (typeof parent === "string" || parent instanceof String ) {
                this.parent = document.getElementById(parent);
                if (parent === null) {
                    throw new Error("Attempted to create chart on non defined element");
                }
            } else {
                throw new Error("Attempted to create chart on unknown element");
            }

            this.options = new Options();

            this.root = root;
            this.currentRoot = root;

            this.scale = document.createElement('canvas');
            this.scale.classList.add('scale');
            this.scale.width = 50;
            this.scale.height = 1000;

            this.scaleCtx = this.scale.getContext('2d');
            this.renderScale();

            this.canvas = document.createElement('canvas');
            this.canvas.classList.add('main');

            let columns = document.createElement('div');
            columns.classList.add('columns');
            columns.classList.add('p-0');
            columns.classList.add('m-0');

            let scaleColumn = document.createElement('div');
            scaleColumn.classList.add('column');
            scaleColumn.classList.add('p-0');
            scaleColumn.classList.add('m-0');
            scaleColumn.classList.add('is-narrow');
            scaleColumn.appendChild(this.scale);

            let mainColumn = document.createElement('div');
            mainColumn.classList.add('column');
            mainColumn.classList.add('p-0');
            mainColumn.classList.add('m-0');
            mainColumn.appendChild(this.canvas);

            columns.appendChild(scaleColumn);
            columns.appendChild(mainColumn);

            this.parent.appendChild(columns);

            this.canvas.width = 1000;
            this.canvas.height = 1000;
            this.center = {x: this.canvas.width / 2, y: this.canvas.height / 2};
            this.radius = 0.95 * this.canvas.width;
            this.firstWidth = this.radius / 8;
            this.secondWidth = this.radius / 3.5;
            this.thirdWidth = this.radius / 2;

            this.ctx = this.canvas.getContext('2d');
            this.fontSize = 20;
            this.ctx.font = this.fontSize + 'px Arial';

            this.canvas.addEventListener('click', e => this.onClick(e));
            this.canvas.addEventListener('auxclick', e => this.onClick(e));
            this.canvas.addEventListener('mousemove', e => this.onMouseMove(e));

            this.listeners = {
                click: [],
                mouseover: [],
                mouseout: [],
            };
        }

        renderScale() {
            this.scaleCtx.clearRect(0, 0, this.scale.width, this.scale.height);
            for (let y = 1; y < 1000; y++) {
                this.scaleCtx.beginPath();
                this.scaleCtx.moveTo(0, y);
                this.scaleCtx.lineTo(this.scale.width, y);
                this.scaleCtx.strokeStyle = interpolateColor(
                    COLOR_BREWER[this.options.colorTheme][0],
                    COLOR_BREWER[this.options.colorTheme][1],
                    1 - y / 1000,
                );
                this.scaleCtx.stroke();
            }

            this.scaleCtx.beginPath();
            this.scaleCtx.rect(1, 1, this.scale.width - 2, this.scale.height - 2);
            this.scaleCtx.strokeStyle = 'white';
            this.scaleCtx.stroke();
        }

        addEventListener(type, callback) {
            if (this.listeners[type] === undefined) {
                throw new Error("Attempted to trigger listener for unknown event type: " + type);
            }

            this.listeners[type].push(callback);
            return this;
        }

        trigger(type, child, event) {
            if (this.listeners[type] === undefined) {
                throw new Error("Attempted to trigger listener for unknwon event type: " + type);
            }

            for (let listener of this.listeners[type]) {
                listener.call(this, child, event);
            }
        }

        render() {
            this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);

            this.ctx.lineWidth = 2;
            this.ctx.strokeStyle = "white";
            this.ctx.fillStyle = "white";

            // Third level
            let currentAngle = 0;
            let total = this.currentRoot.children.map(x => x.children.map(x => x.width(this.options)).reduce((a, b) => a + b, 0)).reduce((a, b) => a + b, 0) / (2 * Math.PI);

            if (this.currentRoot.children.length > 0 && this.currentRoot.children[0].children.length > 0) {

                for (let tmpChild of this.currentRoot.children) {
                    for (let child of tmpChild.children) {
                        // Arc for the child
                        this.ctx.fillStyle = child.color(this.options);
                        this.ctx.beginPath();
                        this.ctx.moveTo(this.center.x, this.center.y);
                        this.ctx.arc(this.center.x, this.center.y, this.thirdWidth, currentAngle, currentAngle + child.width(this.options) / total);
                        this.ctx.closePath();
                        this.ctx.fill();

                        // Draw text
                        let r = (this.secondWidth + this.thirdWidth) / 2;
                        let theta = currentAngle + child.width(this.options) / (2 * total);

                        let x = this.center.x + r * Math.cos(theta);
                        let y = this.center.y + r * Math.sin(theta);

                        this.ctx.fillStyle = "black";
                        let { width } = this.ctx.measureText(child.prettyName());

                        this.ctx.save();
                        this.ctx.translate(this.center.x, this.center.y);

                        let angle = currentAngle + child.width(this.options) / (2 * total);
                        let reverse = (angle + Math.PI / 2) % (2 * Math.PI) > Math.PI;

                        this.ctx.rotate(angle + (reverse ? -0.025 : 0.025));
                        this.ctx.translate(r, 0);

                        if (reverse) {
                            this.ctx.rotate(Math.PI);
                        }

                        this.ctx.fillText(child.prettyName(), -width / 2, 0);
                        this.ctx.restore();

                        currentAngle += child.width(this.options) / total;
                    }
                }

                // Third level lines
                this.ctx.fillStyle = "white";
                currentAngle = 0;

                for (let tmpChild of this.currentRoot.children) {
                    for (let child of tmpChild.children) {
                        // Arc for the child
                        this.ctx.beginPath();
                        this.ctx.moveTo(this.center.x, this.center.y);
                        this.ctx.arc(this.center.x, this.center.y, this.thirdWidth, currentAngle, currentAngle + child.width(this.options) / total);
                        this.ctx.closePath();
                        this.ctx.stroke();

                        currentAngle += child.width(this.options) / total;
                    }
                }

            }

            // Second level
            currentAngle = 0;
            total = this.currentRoot.children.map(x => x.width(this.options)).reduce((a, b) => a + b, 0) / (2 * Math.PI);

            // Increase width if last level
            let localWidth = this.secondWidth;

            if (this.currentRoot.children.length > 0 && this.currentRoot.children[0].children.length === 0) {
                localWidth = this.thirdWidth;
            }

            for (let child of this.currentRoot.children) {
                // Arc for the child
                this.ctx.fillStyle = child.color(this.options);
                this.ctx.beginPath();
                this.ctx.moveTo(this.center.x, this.center.y);
                this.ctx.arc(this.center.x, this.center.y, localWidth, currentAngle, currentAngle + child.width(this.options) / total);
                this.ctx.closePath();
                this.ctx.fill();

                // Draw text
                let r = (this.firstWidth + localWidth)  / 2;
                let theta = currentAngle + child.width(this.options) / (2 * total);

                let x = this.center.x + r * Math.cos(theta);
                let y = this.center.y + r * Math.sin(theta);

                this.ctx.fillStyle = "black";
                let { width } = this.ctx.measureText(child.prettyName());
                this.ctx.save();
                this.ctx.translate(this.center.x, this.center.y);


                let angle = currentAngle + child.width(this.options) / (2 * total);
                let reverse = (angle + Math.PI / 2) % (2 * Math.PI) > Math.PI;
                this.ctx.rotate(angle + (reverse ? -0.025 : 0.025));
                this.ctx.translate(r, 0);

                if (reverse) {
                    this.ctx.rotate(Math.PI);
                }

                this.ctx.fillText(child.prettyName(), -width / 2, 0);
                this.ctx.restore();


                currentAngle += child.width(this.options) / total;
            }

            // Second level lines
            this.ctx.fillStyle = "white";
            currentAngle = 0;

            for (let child of this.currentRoot.children) {
                // Arc for the child
                this.ctx.beginPath();
                this.ctx.moveTo(this.center.x, this.center.y);
                this.ctx.arc(this.center.x, this.center.y, localWidth, currentAngle, currentAngle + child.width(this.options) / total);
                this.ctx.closePath();
                this.ctx.stroke();

                currentAngle += child.width(this.options) / total;
            }


            // First level
            this.ctx.fillStyle = this.currentRoot.color(this.options);
            this.ctx.beginPath();
            this.ctx.arc(this.center.x, this.center.y, this.firstWidth, 0, 2 * Math.PI, true);
            this.ctx.fill();

            this.ctx.fillStyle = "black";
            let { width } = this.ctx.measureText(this.currentRoot.prettyName());
            this.ctx.fillText(this.currentRoot.prettyName(), this.center.x - width / 2, this.center.y);

            // First level lines
            this.ctx.beginPath();
            this.ctx.arc(this.center.x, this.center.y, this.firstWidth, 0, 2 * Math.PI, true);
            this.ctx.stroke();
        }

        getElement(x, y) {
            let theta = Math.atan2(y, x);

            // Put theta between 0 and 2*pi
            if (theta < 0) {
                theta += 2 * Math.PI;
            }

            let r2 = x*x + y*y;

            if (r2 < this.firstWidth * this.firstWidth) {
                return this.currentRoot;
            }

            // Increase width if last level
            let localWidth = this.secondWidth;

            if (this.currentRoot.children.length > 0 && this.currentRoot.children[0].children.length === 0) {
                localWidth = this.thirdWidth;
            }

            if (r2 < localWidth * localWidth) {

                let currentAngle = 0;
                let total = this.currentRoot.children.map(x => x.width(this.options)).reduce((a, b) => a + b, 0) / (2 * Math.PI);

                for (let child of this.currentRoot.children) {
                    currentAngle += child.width(this.options) / total;

                    if (theta < currentAngle) {
                        return child;
                    }
                }
            }

            if (r2 < this.thirdWidth * this.thirdWidth) {

                let currentAngle = 0;
                let total = this.currentRoot.children.map(x => x.children.map(x => x.width(this.options)).reduce((a, b) => a + b, 0)).reduce((a, b) => a + b, 0) / (2 * Math.PI);

                for (let tmpChild of this.currentRoot.children) {
                    for (let child of tmpChild.children) {
                        currentAngle += child.width(this.options) / total;

                        if (theta < currentAngle) {
                            return child;
                        }
                    }
                }
            }
        }

        getCurrentHovering() {
            if (this.currentRoot.hovering) {
                return this.currentRoot;
            }

            for (let child of this.currentRoot.children) {
                if (child.hovering) {
                    return child;
                }
            }

            for (let tmpChild of this.currentRoot.children) {
                for (let child of tmpChild.children) {
                    if (child.hovering) {
                        return child;
                    }
                }
            }

        }

        onClick(event) {
            let child = this.getElement(
                event.offsetX * this.canvas.width / this.canvas.offsetWidth - this.center.x,
                event.offsetY * this.canvas.height / this.canvas.offsetHeight- this.center.y
            );

            if (child !== undefined) {
                this.trigger('click', child, event);
            }
        }

        onMouseMove(event) {
            let currentHovering = this.getCurrentHovering();
            let nextHovering = this.getElement(
                event.offsetX * this.canvas.width / this.canvas.offsetWidth - this.center.x,
                event.offsetY * this.canvas.height / this.canvas.offsetHeight - this.center.y
            );

            if (currentHovering !== nextHovering) {
                if (currentHovering !== undefined) {
                    currentHovering.hovering = false;
                    this.trigger('mouseout', currentHovering, event);
                }

                if (nextHovering) {
                    nextHovering.hovering = true;
                    this.trigger('mouseover', nextHovering, event);
                }
            }
        }
    }

    function generateInfo(tree) {
        // Root element
        let info = document.createElement('div');

        let columns = document.createElement('div');
        columns.classList.add('columns');

        // Tax part
        let taxColumn = document.createElement('div');
        taxColumn.classList.add('column');

        // Title for taxonomy part of info
        let taxTitle = document.createElement('h3');
        taxTitle.innerHTML = "Taxonomie";
        taxColumn.appendChild(taxTitle);

        // Hierarchy of taxonomy
        let tax = document.createElement('ul');

        let hier = [];
        let iter = tree;

        while (iter !== null) {
            hier.push(iter);
            iter = iter.parent;
        }

        for (let i = hier.length - 1; i >= 0; i--) {
            let current = hier[i];

            let item = document.createElement('li');

            let link = document.createElement('a');
            link.innerHTML = current.name;
            link.setAttribute('href', '/species/' + depthToTaxon(current.depth) + '/' + current.name + '/1');
            item.appendChild(link);

            tax.appendChild(item);
        }

        taxColumn.appendChild(tax);

        // All metadata
        let tableColumn = document.createElement('div');
        tableColumn.classList.add('column');

        let metadataTitle = document.createElement('h3');
        metadataTitle.innerHTML = 'Metadonnées';
        tableColumn.appendChild(metadataTitle);

        let table = document.createElement('table');
        let body  = document.createElement('tbody');

        for (let key in tree.metadata) {

            if (['id', 'reign', 'phylum', 'class', 'order', 'family', 'genus', 'species', 'example_media_path'].indexOf(key) === -1) {
                let row = document.createElement('tr');

                let rowHeading = document.createElement('th');
                rowHeading.innerHTML = key;

                let rowCell = document.createElement('td');
                rowCell.classList.add('has-text-right');
                rowCell.innerHTML = tree.metadata[key].toLocaleString();

                row.appendChild(rowHeading);
                row.appendChild(rowCell);

                body.appendChild(row);
            }

        }

        table.appendChild(body);
        tableColumn.appendChild(table);

        columns.appendChild(taxColumn);
        columns.appendChild(tableColumn);

        info.appendChild(columns);

        return info;
    }

    function generateExample(tree) {
        let exampleTitle = document.getElementById('example-title');
        exampleTitle.style.display = "block";

        let example = document.getElementById('example');
        example.setAttribute('src', '/data/medias/' + tree.metadata.example_media_path);
    }

    function generateControls(chart) {
        let controls = document.getElementById('controls');

        let colorThemeTitle = document.createElement('h3');
        colorThemeTitle.innerHTML = "Thème de couleur";
        controls.appendChild(colorThemeTitle);

        let colorThemeSelect = document.createElement('select');
        colorThemeSelect.classList.add('select');

        for (let key in COLOR_BREWER) {
            let option = document.createElement('option');
            option.setAttribute('value', key);

            if (key === chart.options._colorTheme) {
                option.setAttribute('selected', 'selected');
            }

            option.innerHTML = key;
            colorThemeSelect.appendChild(option);
        }

        colorThemeSelect.addEventListener('change', event => {
            chart.options.colorTheme = event.target.value;
            chart.renderScale();
            chart.render();
        });

        controls.appendChild(colorThemeSelect);

        let widthTitle = document.createElement('h3');
        widthTitle.innerHTML = "Taille des cercles";
        controls.appendChild(widthTitle);

        let widthSelect = document.createElement('select');
        widthSelect.classList.add('select');

        for (let stat of Stat.all()) {
            let option = document.createElement('option');
            option.setAttribute('value', stat.name);

            if (stat.name === chart.options.widthStat.name) {
                option.setAttribute('selected', 'selected');
            }

            option.innerHTML = stat.name;
            widthSelect.appendChild(option);
        }

        widthSelect.addEventListener('change', event => {
            chart.options.widthStat = event.target.value;
            chart.render();
        });

        controls.appendChild(widthSelect);

        let colorTitle = document.createElement('h3');
        colorTitle.innerHTML = "Couleur des cercles";
        controls.appendChild(colorTitle);

        let colorSelect = document.createElement('select');
        colorSelect.classList.add('select');

        for (let stat of Stat.all()) {
            let option = document.createElement('option');
            option.setAttribute('value', stat.name);

            if (stat.name === chart.options.colorStat.name) {
                option.setAttribute('selected', 'selected');
            }

            option.innerHTML = stat.name;
            colorSelect.appendChild(option);
        }

        colorSelect.addEventListener('change', event => {
            chart.options.colorStat = event.target.value;
            chart.render();
        });

        controls.appendChild(colorSelect);
    }

    async function main() {
        let infoElement = document.getElementById('info');
        let infoChild = null;

        let chart = new Chart("sunburst");
        generateControls(chart);

        async function loadFromLocation() {
            let currentPath = window.location.hash.slice(1).split('=');
            let currentTaxon = null, currentValue = null, currentDepth = null;

            try {
                currentTaxon = currentPath[0];
                currentDepth = taxonToDepth(currentTaxon); // will throw if current taxon does not exist
                currentValue = currentPath[1];
            } catch {
                currentTaxon = 'reign';
                currentValue = 'Animalia';
                currentDepth = 0;
            }

            let json = await fetch(`/plotly/${currentTaxon}/${currentValue}`);
            json = await json.json();

            let tree = Tree.fromJson(json);
            chart.root = tree;
            chart.currentRoot = tree;
            chart.render();
        }

        window.addEventListener('popstate', async event => {
            loadFromLocation();
        });

        chart.addEventListener('click', async function(child, event) {
            // If ctrl key is pressed down, or mouse wheel click, open list of species in new tab
            if (event.type === "auxclick" || event.ctrlKey) {
                window.open('/species/' + depthToTaxon(child.depth) + '/' + child.name + '/1');
                return;
            }
            if (chart.currentRoot === child) {
                if (child.depth > 0) {
                    window.location = "#" + depthToTaxon(child.depth - 1) + "=" + child.metadata[depthToTaxon(child.depth - 1)];
                }
            } else {
                window.location = "#" + depthToTaxon(child.depth) + "=" + child.name;
            }
            chart.render();
        });

        chart.addEventListener('mouseover', child => {
            let generated = generateInfo(child);

            if (infoChild === null) {
                infoElement.appendChild(generated);
            } else {
                infoElement.replaceChild(generated, infoChild);
            }

            generateExample(child);

            infoChild = generated;
        });

        loadFromLocation();
    }

    return { Tree, Chart, depthToTaxon, main };

}

window.Plot = (function() {
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

    class Tree {
        constructor(name, depth = 0) {
            colorCounter++;

            this.parent = null;
            this.name = name;
            this.children = [];
            this.hovering = false;
            this.depth = depth;
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

        width() {
            return this.metadata.medias_downloaded_count || 1;
        }

        // Returns a float between 0 and 1
        colorValue() {
            return this.metadata.medias_cropped_count / this.metadata.medias_downloaded_count;
        }

        // Returns a canvas ready color from colorValue
        color() {
            let value = this.colorValue();

            let r = Math.round((1 - value) * 255);
            let g = Math.round(value * 255);
            let b = 0;

            return `rgb(${r}, ${g}, ${b})`;
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

            this.root = root;
            this.currentRoot = root;

            this.canvas = document.createElement('canvas');
            this.parent.appendChild(this.canvas);

            this.canvas.width = 1000;
            this.canvas.height = 1000;
            this.center = {x: this.canvas.width / 2, y: this.canvas.height / 2};
            this.radius = 0.95 * this.canvas.width;
            this.firstWidth = this.radius / 10;
            this.secondWidth = this.radius / 4;
            this.thirdWidth = this.radius / 2.5;

            this.ctx = this.canvas.getContext('2d');
            this.fontSize = 20;
            this.ctx.font = this.fontSize + 'px Verdana';

            this.canvas.addEventListener('click', e => this.onClick(e));
            this.canvas.addEventListener('auxclick', e => this.onClick(e));
            this.canvas.addEventListener('mousemove', e => this.onMouseMove(e));

            this.listeners = {
                click: [],
                mouseover: [],
                mouseout: [],
            };
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
            let total = this.currentRoot.children.map(x => x.children.map(x => x.width()).reduce((a, b) => a + b, 0)).reduce((a, b) => a + b, 0) / (2 * Math.PI);

            if (this.currentRoot.children.length > 0 && this.currentRoot.children[0].children.length > 0) {

                for (let tmpChild of this.currentRoot.children) {
                    for (let child of tmpChild.children) {
                        // Arc for the child
                        this.ctx.fillStyle = child.color();
                        this.ctx.beginPath();
                        this.ctx.moveTo(this.center.x, this.center.y);
                        this.ctx.arc(this.center.x, this.center.y, this.thirdWidth, currentAngle, currentAngle + child.width() / total);
                        this.ctx.closePath();
                        this.ctx.fill();

                        // Draw text
                        let r = (this.secondWidth + this.thirdWidth) / 2;
                        let theta = currentAngle + child.width() / (2 * total);

                        let x = this.center.x + r * Math.cos(theta);
                        let y = this.center.y + r * Math.sin(theta);

                        this.ctx.fillStyle = "black";
                        let { width } = this.ctx.measureText(child.name);

                        this.ctx.save();
                        this.ctx.translate(this.center.x, this.center.y);

                        let angle = currentAngle + child.width() / (2 * total);
                        let reverse = (angle + Math.PI / 2) % (2 * Math.PI) > Math.PI;

                        this.ctx.rotate(angle + (reverse ? -0.025 : 0.025));
                        this.ctx.translate(r, 0);

                        if (reverse) {
                            this.ctx.rotate(Math.PI);
                        }

                        this.ctx.fillText(child.name, -width / 2, 0);
                        this.ctx.restore();

                        // this.ctx.fillText(child.name, x - width / 2, y);


                        currentAngle += child.width() / total;
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
                        this.ctx.arc(this.center.x, this.center.y, this.thirdWidth, currentAngle, currentAngle + child.width() / total);
                        this.ctx.closePath();
                        this.ctx.stroke();

                        currentAngle += child.width() / total;
                    }
                }

            }

            // Second level
            currentAngle = 0;
            total = this.currentRoot.children.map(x => x.width()).reduce((a, b) => a + b, 0) / (2 * Math.PI);

            // Increase width if last level
            let localWidth = this.secondWidth;

            if (this.currentRoot.children.length > 0 && this.currentRoot.children[0].children.length === 0) {
                localWidth = this.thirdWidth;
            }

            for (let child of this.currentRoot.children) {
                // Arc for the child
                this.ctx.fillStyle = child.color();
                this.ctx.beginPath();
                this.ctx.moveTo(this.center.x, this.center.y);
                this.ctx.arc(this.center.x, this.center.y, localWidth, currentAngle, currentAngle + child.width() / total);
                this.ctx.closePath();
                this.ctx.fill();

                // Draw text
                let r = (this.firstWidth + localWidth)  / 2;
                let theta = currentAngle + child.width() / (2 * total);

                let x = this.center.x + r * Math.cos(theta);
                let y = this.center.y + r * Math.sin(theta);

                this.ctx.fillStyle = "black";
                let { width } = this.ctx.measureText(child.name);
                this.ctx.save();
                this.ctx.translate(this.center.x, this.center.y);


                let angle = currentAngle + child.width() / (2 * total);
                let reverse = (angle + Math.PI / 2) % (2 * Math.PI) > Math.PI;
                this.ctx.rotate(angle + (reverse ? -0.025 : 0.025));
                this.ctx.translate(r, 0);

                if (reverse) {
                    this.ctx.rotate(Math.PI);
                }

                this.ctx.fillText(child.name, -width / 2, 0);
                this.ctx.restore();


                currentAngle += child.width() / total;
            }

            // Second level lines
            this.ctx.fillStyle = "white";
            currentAngle = 0;

            for (let child of this.currentRoot.children) {
                // Arc for the child
                this.ctx.beginPath();
                this.ctx.moveTo(this.center.x, this.center.y);
                this.ctx.arc(this.center.x, this.center.y, localWidth, currentAngle, currentAngle + child.width() / total);
                this.ctx.closePath();
                this.ctx.stroke();

                currentAngle += child.width() / total;
            }


            // First level
            this.ctx.fillStyle = "black";
            this.ctx.beginPath();
            this.ctx.arc(this.center.x, this.center.y, this.firstWidth, 0, 2 * Math.PI, true);
            this.ctx.fill();

            this.ctx.fillStyle = "white";
            let { width } = this.ctx.measureText(this.currentRoot.name);
            this.ctx.fillText(this.currentRoot.name, this.center.x - width / 2, this.center.y);

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
                let total = this.currentRoot.children.map(x => x.width()).reduce((a, b) => a + b, 0) / (2 * Math.PI);

                for (let child of this.currentRoot.children) {
                    currentAngle += child.width() / total;

                    if (theta < currentAngle) {
                        return child;
                    }
                }
            }

            if (r2 < this.thirdWidth * this.thirdWidth) {

                let currentAngle = 0;
                let total = this.currentRoot.children.map(x => x.children.map(x => x.width()).reduce((a, b) => a + b, 0)).reduce((a, b) => a + b, 0) / (2 * Math.PI);

                for (let tmpChild of this.currentRoot.children) {
                    for (let child of tmpChild.children) {
                        currentAngle += child.width() / total;

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
            let child = this.getElement(event.offsetX - this.center.x, event.offsetY - this.center.y);
            if (child !== undefined) {
                this.trigger('click', child, event);
            }
        }

        onMouseMove(event) {
            let currentHovering = this.getCurrentHovering();
            let nextHovering = this.getElement(event.offsetX - this.center.x, event.offsetY - this.center.y);

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

    return { Tree, Chart, depthToTaxon };
})();

export function rgbToCssColor(r:number, g:number, b:number) {
    return `rgb(${r}, ${g}, ${b})`;
}

export function cssVariables(node: HTMLDivElement, variables: { basecolor: string; }) {
    setCssVariables(node, variables);
    return { update(variables: any) { setCssVariables(node, variables); } }
}

export function setCssVariables(node: HTMLDivElement, variables: { [x: string]: any; basecolor?: string; }) {
    for (const name in variables) {
        node.style.setProperty(`--${name}`, variables[name]);
    }
}

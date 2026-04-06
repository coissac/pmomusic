declare module 'vue-virtual-scroller' {
    import { DefineComponent } from 'vue';

    export interface RecycleScrollerProps {
        items?: any[];
        itemSize?: number;
        keyField?: string;
        direction?: 'vertical' | 'horizontal';
        minItemSize?: number;
        sizeField?: string;
        typeField?: string;
        buffer?: number;
        pageMode?: boolean;
        prerender?: number;
    }

    export interface RecycleScrollerRef {
        scrollToItem(index: number): void;
        scrollToOffset(offset: number): void;
        $el: HTMLElement;
    }

    export const RecycleScroller: DefineComponent<RecycleScrollerProps>;
}
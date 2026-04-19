<template>
  <div class="relative flex h-full min-h-0 flex-col gap-3">
    <div class="shrink-0 pr-4 pt-4">
      <slot name="header" />
    </div>

    <div class="scrollbar-gutter-stable min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-0 pb-24">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.scrollbar-gutter-stable {
  scrollbar-gutter: stable;
}
</style>

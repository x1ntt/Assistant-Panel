<script setup lang="ts">
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Pin } from "lucide-vue-next";
</script>

<template>
  <div class="p-2 space-y-2">
    <div class="flex items-center gap-1.5 mb-2">
      <Pin class="h-4 w-4" />
      <h2 class="text-sm font-semibold">窗口置顶</h2>
    </div>
    <Card>
      <CardHeader class="py-1 px-2">
        <CardTitle class="text-xs">窗口列表</CardTitle>
      </CardHeader>
      <CardContent class="px-2 pb-2">
        <p class="text-xs text-muted-foreground">暂无置顶窗口，选择窗口进行置顶</p>
        <Button class="mt-2" size="sm" variant="outline">刷新窗口列表</Button>
      </CardContent>
    </Card>
  </div>
</template>

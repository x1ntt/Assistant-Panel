<script setup lang="ts">
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Network } from "lucide-vue-next";
</script>

<template>
  <div class="p-2 space-y-2">
    <div class="flex items-center gap-1.5 mb-2">
      <Network class="h-4 w-4" />
      <h2 class="text-sm font-semibold">端口转发</h2>
    </div>
    <Card>
      <CardHeader class="py-1 px-2">
        <CardTitle class="text-xs">转发规则</CardTitle>
      </CardHeader>
      <CardContent class="px-2 pb-2">
        <p class="text-xs text-muted-foreground">暂无转发规则</p>
        <Button class="mt-2" size="sm" variant="outline">添加规则</Button>
      </CardContent>
    </Card>
  </div>
</template>

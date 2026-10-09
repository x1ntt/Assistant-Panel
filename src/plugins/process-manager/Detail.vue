<script setup lang="ts">
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Activity } from "lucide-vue-next";
</script>

<template>
  <div class="p-4 space-y-4">
    <div class="flex items-center gap-2 mb-4">
      <Activity class="h-5 w-5" />
      <h2 class="text-lg font-semibold">Process Manager</h2>
    </div>
    <Card>
      <CardHeader>
        <CardTitle class="text-sm">Processes</CardTitle>
      </CardHeader>
      <CardContent>
        <p class="text-sm text-muted-foreground">No processes configured. Add a process to manage.</p>
        <Button class="mt-4" size="sm">Add Process</Button>
      </CardContent>
    </Card>
  </div>
</template>

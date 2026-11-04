package com.kylejameswalker.lastharvest;

import android.app.NativeActivity;
import android.app.Presentation;
import android.content.ContentResolver;
import android.content.Context;
import android.content.Intent;
import android.content.UriPermission;
import android.database.Cursor;
import android.net.Uri;
import android.provider.DocumentsContract;
import android.provider.DocumentsContract.Document;
import android.widget.Toast;
import android.media.AudioAttributes;
import android.graphics.PixelFormat;
import android.hardware.display.DisplayManager;
import android.os.Build;
import android.os.Bundle;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.view.Display;
import android.view.MotionEvent;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * The game's NativeActivity, with what the NDK can't reach: the vibrator, and a second screen, which Android shows
 * as a separate presentation display. The bottom screen draws into a SurfaceView on that display, and its touches go
 * to the game as bottom-screen touches. The game's window keeps the system bars hidden, as a swipe brings them back
 * only for a moment. It also keeps the game folder the player chose with Android's folder picker: mods and cards load
 * from it, and screenshots go into it.
 */
public class MainActivity extends NativeActivity implements DisplayManager.DisplayListener {
    static {
        // NativeActivity loads the library itself, but JNI only finds native methods in ones loaded here.
        System.loadLibrary("last_harvest");
    }

    /** The second screen's window and its touches, which android.rs takes. A null surface takes it away. */
    static native void secondSurface(Surface surface);

    /** A finger on the second screen: phase 0 down, 1 moved, 2 up; x and y from 0 to 1 across it. */
    static native void secondTouch(int phase, int id, float x, float y);

    /** The pack fetchPack asked for has arrived, or failed with this error. */
    static native void packReady(String error);

    /** The player chose this game folder, by the name folderName gives it. */
    static native void folderChosen(String name);

    private static final int PICK_FOLDER = 1;
    private static final String FOLDER = "folder";
    /** The game folder's subfolders that copyFolder copies, as the handheld has them beside the binary. */
    private static final String[] DATA = {"mods", "enemies", "players", "props", "maps"};
    private static final String SHOTS = "screenshots";

    /** Screenshots taken before there was a game folder, saved once the picker returns one: kind, then the PNGs. */
    private final List<Object[]> waiting = new ArrayList<>();

    private Presentation presentation;
    private DisplayManager displays;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            WindowManager.LayoutParams layout = getWindow().getAttributes();
            layout.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            getWindow().setAttributes(layout);
        }
        hideSystemBars();
        displays = (DisplayManager) getSystemService(Context.DISPLAY_SERVICE);
        displays.registerDisplayListener(this, null);
        showSecondScreen();
    }

    @Override
    protected void onResume() {
        super.onResume();
        hideSystemBars();
    }

    @Override
    public void onWindowFocusChanged(boolean focused) {
        super.onWindowFocusChanged(focused);
        if (focused) {
            hideSystemBars();
        }
    }

    /** Sticky immersive full screen: a swipe shows the status and navigation bars, and they hide again. */
    private void hideSystemBars() {
        Window window = getWindow();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            window.setDecorFitsSystemWindows(false);
            WindowInsetsController bars = window.getInsetsController();
            if (bars != null) {
                bars.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                bars.hide(WindowInsets.Type.systemBars());
            }
        } else {
            window.getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION);
        }
    }

    @Override
    protected void onDestroy() {
        displays.unregisterDisplayListener(this);
        hideSecondScreen();
        super.onDestroy();
    }

    @Override
    public void onDisplayAdded(int id) {
        showSecondScreen();
    }

    @Override
    public void onDisplayRemoved(int id) {
        if (presentation != null && presentation.getDisplay().getDisplayId() == id) {
            hideSecondScreen();
        }
    }

    @Override
    public void onDisplayChanged(int id) {}

    private void showSecondScreen() {
        if (presentation != null) {
            return;
        }
        Display[] found = displays.getDisplays(DisplayManager.DISPLAY_CATEGORY_PRESENTATION);
        if (found.length == 0) {
            return;
        }
        presentation = new Presentation(this, found[0]);
        SurfaceView view = new SurfaceView(presentation.getContext());
        view.getHolder().setFormat(PixelFormat.RGBA_8888);
        view.getHolder().addCallback(new SurfaceHolder.Callback() {
            @Override
            public void surfaceCreated(SurfaceHolder holder) {}

            @Override
            public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
                secondSurface(holder.getSurface());
            }

            @Override
            public void surfaceDestroyed(SurfaceHolder holder) {
                secondSurface(null);
            }
        });
        view.setOnTouchListener((v, e) -> {
            float w = Math.max(1, v.getWidth()), h = Math.max(1, v.getHeight());
            int action = e.getActionMasked();
            if (action == MotionEvent.ACTION_MOVE) {
                for (int i = 0; i < e.getPointerCount(); i++) {
                    secondTouch(1, e.getPointerId(i), e.getX(i) / w, e.getY(i) / h);
                }
                return true;
            }
            int phase = switch (action) {
                case MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> 0;
                case MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP, MotionEvent.ACTION_CANCEL -> 2;
                default -> -1;
            };
            if (phase >= 0) {
                int i = e.getActionIndex();
                secondTouch(phase, e.getPointerId(i), e.getX(i) / w, e.getY(i) / h);
            }
            return true;
        });
        presentation.setContentView(view);
        // Touches still reach it, but the gamepad's keys stay with the game's window.
        presentation.getWindow().addFlags(WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE);
        presentation.show();
    }

    private void hideSecondScreen() {
        if (presentation != null) {
            presentation.dismiss();
            presentation = null;
        }
    }

    /** Called from android.rs when the APK carries no game_data.pak: fetches url to path off the UI thread. */
    public void fetchPack(String url, String path) {
        new Thread(() -> {
            File out = new File(path), part = new File(path + ".part");
            String error = null;
            try {
                HttpURLConnection c = (HttpURLConnection) new URL(url).openConnection();
                c.setConnectTimeout(10000);
                c.setReadTimeout(30000);
                if (c.getResponseCode() != HttpURLConnection.HTTP_OK) {
                    throw new IOException("HTTP " + c.getResponseCode() + " for " + url);
                }
                try (InputStream in = c.getInputStream(); OutputStream to = new FileOutputStream(part)) {
                    byte[] buffer = new byte[65536];
                    for (int n; (n = in.read(buffer)) > 0; ) {
                        to.write(buffer, 0, n);
                    }
                }
                if (!part.renameTo(out)) {
                    throw new IOException("can't move " + part + " to " + out);
                }
            } catch (Exception e) {
                error = e.toString();
                part.delete();
            }
            packReady(error);
        }).start();
    }

    /** Called from android.rs while the game's rumble is on. */
    public void vibrate(int ms) {
        Vibrator v = (Vibrator) getSystemService(Context.VIBRATOR_SERVICE);
        if (v != null && v.hasVibrator()) {
            AudioAttributes game = new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME).build();
            v.vibrate(VibrationEffect.createOneShot(ms, VibrationEffect.DEFAULT_AMPLITUDE), game);
        }
    }

    /** Called from android.rs when the game's rumble stops. */
    public void stopVibrate() {
        Vibrator v = (Vibrator) getSystemService(Context.VIBRATOR_SERVICE);
        if (v != null) {
            v.cancel();
        }
    }

    /** The chosen game folder, while Android still lets the game read and write it; else null. */
    private Uri folder() {
        String saved = getPreferences(MODE_PRIVATE).getString(FOLDER, null);
        if (saved == null) {
            return null;
        }
        Uri uri = Uri.parse(saved);
        for (UriPermission p : getContentResolver().getPersistedUriPermissions()) {
            if (p.getUri().equals(uri) && p.isReadPermission() && p.isWritePermission()) {
                return uri;
            }
        }
        return null;
    }

    /** Called from android.rs: the game folder as Settings names it, such as Documents/Last Harvest, or "". */
    public String folderName() {
        Uri tree = folder();
        if (tree == null) {
            return "";
        }
        String id = DocumentsContract.getTreeDocumentId(tree);
        String path = id.substring(id.indexOf(':') + 1);
        return path.isEmpty() ? "Storage root" : path;
    }

    /** Called from android.rs when Settings asks: shows Android's folder picker. */
    public void pickFolder() {
        runOnUiThread(() -> {
            Intent pick = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
            pick.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
            startActivityForResult(pick, PICK_FOLDER);
        });
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != PICK_FOLDER) {
            return;
        }
        Uri tree = result == RESULT_OK && data != null ? data.getData() : null;
        if (tree == null) {
            synchronized (waiting) {
                waiting.clear();
            }
            return;
        }
        ContentResolver resolver = getContentResolver();
        resolver.takePersistableUriPermission(tree, Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
        for (UriPermission p : resolver.getPersistedUriPermissions()) {
            if (!p.getUri().equals(tree)) {
                resolver.releasePersistableUriPermission(p.getUri(), Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            }
        }
        getPreferences(MODE_PRIVATE).edit().putString(FOLDER, tree.toString()).apply();
        String name = folderName();
        folderChosen(name);
        Toast.makeText(this, "Game folder: " + name + ". Mods and cards in it load the next time the game starts.", Toast.LENGTH_LONG).show();
        new Thread(() -> {
            try {
                String root = DocumentsContract.getTreeDocumentId(tree);
                for (String sub : DATA) {
                    folderIn(tree, root, sub);
                }
                folderIn(tree, root, SHOTS);
            } catch (Exception e) {
                android.util.Log.w("last-harvest", "game folder: " + e);
            }
            List<Object[]> shots;
            synchronized (waiting) {
                shots = new ArrayList<>(waiting);
                waiting.clear();
            }
            for (Object[] shot : shots) {
                saveShot((String) shot[0], (byte[]) shot[1], (byte[]) shot[2]);
            }
        }).start();
    }

    /** The documents in folder `parent` of `tree`, by name, with their ids and whether each is a folder. */
    private Map<String, String[]> children(Uri tree, String parent) {
        Map<String, String[]> found = new HashMap<>();
        Uri list = DocumentsContract.buildChildDocumentsUriUsingTree(tree, parent);
        String[] columns = {Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME, Document.COLUMN_MIME_TYPE};
        try (Cursor c = getContentResolver().query(list, columns, null, null, null)) {
            while (c != null && c.moveToNext()) {
                found.put(c.getString(1), new String[] {c.getString(0), c.getString(2)});
            }
        }
        return found;
    }

    /** Folder `name` in folder `parent` of `tree`, made if it isn't there, as its document id. */
    private String folderIn(Uri tree, String parent, String name) throws IOException {
        String[] there = children(tree, parent).get(name);
        if (there != null && Document.MIME_TYPE_DIR.equals(there[1])) {
            return there[0];
        }
        Uri made = DocumentsContract.createDocument(getContentResolver(), DocumentsContract.buildDocumentUriUsingTree(tree, parent), Document.MIME_TYPE_DIR, name);
        if (made == null) {
            throw new IOException("can't make " + name);
        }
        return DocumentsContract.getDocumentId(made);
    }

    /**
     * Called from android.rs before the game loads its data: copies the game folder's mods, enemies, players, props
     * and maps folders into `dest`, replacing what an earlier start copied there. Returns false when there is no game
     * folder, and android.rs uses the app's own folder instead.
     */
    public boolean copyFolder(String dest) {
        Uri tree = folder();
        if (tree == null) {
            return false;
        }
        try {
            String root = DocumentsContract.getTreeDocumentId(tree);
            ContentResolver resolver = getContentResolver();
            for (String sub : DATA) {
                File to = new File(dest, sub);
                File[] old = to.listFiles();
                if (old != null) {
                    for (File f : old) {
                        f.delete();
                    }
                }
                to.mkdirs();
                for (Map.Entry<String, String[]> e : children(tree, folderIn(tree, root, sub)).entrySet()) {
                    if (Document.MIME_TYPE_DIR.equals(e.getValue()[1]) || e.getKey().contains("/")) {
                        continue;
                    }
                    Uri doc = DocumentsContract.buildDocumentUriUsingTree(tree, e.getValue()[0]);
                    try (InputStream in = resolver.openInputStream(doc)) {
                        Files.copy(in, new File(to, e.getKey()).toPath());
                    }
                }
            }
            folderIn(tree, root, SHOTS);
            return true;
        } catch (Exception e) {
            android.util.Log.w("last-harvest", "game folder: " + e);
            return false;
        }
    }

    /**
     * Called from android.rs off the game's thread: saves a screenshot to the game folder's screenshots folder as the
     * next unused kind-NNNN.png, and a photo's square as kind-NNNN-square.png. With no game folder yet, the picker
     * asks for one first.
     */
    public void saveShot(String kind, byte[] png, byte[] square) {
        Uri tree = folder();
        if (tree == null) {
            synchronized (waiting) {
                if (waiting.isEmpty()) {
                    pickFolder();
                }
                waiting.add(new Object[] {kind, png, square});
            }
            return;
        }
        try {
            String shots = folderIn(tree, DocumentsContract.getTreeDocumentId(tree), SHOTS);
            Map<String, String[]> there = children(tree, shots);
            String stem = null;
            for (int n = 1; n < 10000 && stem == null; n++) {
                String s = String.format("%s-%04d", kind, n);
                stem = there.containsKey(s + ".png") ? null : s;
            }
            if (stem == null) {
                throw new IOException("no free name for " + kind);
            }
            write(tree, shots, stem + ".png", png);
            if (square != null) {
                write(tree, shots, stem + "-square.png", square);
            }
            android.util.Log.i("last-harvest", "screenshot: " + folderName() + "/" + SHOTS + "/" + stem + ".png");
        } catch (Exception e) {
            android.util.Log.w("last-harvest", "screenshot: " + e);
        }
    }

    private void write(Uri tree, String parent, String name, byte[] bytes) throws IOException {
        Uri made = DocumentsContract.createDocument(getContentResolver(), DocumentsContract.buildDocumentUriUsingTree(tree, parent), "image/png", name);
        if (made == null) {
            throw new IOException("can't make " + name);
        }
        try (OutputStream out = getContentResolver().openOutputStream(made)) {
            out.write(bytes);
        }
    }
}
